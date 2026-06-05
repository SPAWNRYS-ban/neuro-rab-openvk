mod ai;
mod config;
mod context;
mod db;
mod logger;
mod longpoll_manager;
mod openvk;
mod web;

use ai::ClaudeAI;
use anyhow::Result;
use config::{Config, BotMode};
use context::{ContextManager, MentionDetector};
use db::Database;
use log::{error, info, warn};
use openvk::{OpenVKClient, ParsedNotification, Comment, Notification};

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use web::{DuckDuckGoSearch, WebScraper};

/// Safely truncate a string to at most `max_chars` CHARACTERS (not bytes).
///
/// Rust string slicing (`&s[..n]`) panics if `n` falls inside a multi-byte
/// UTF-8 character — which it does constantly with Cyrillic text. This helper
/// truncates on a character boundary so it can NEVER panic. Used for log
/// previews and for trimming responses to the OpenVK length limit.
fn truncate_str(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Compute a STABLE deduplication id for a notification.
///
/// We only handle `mention` (post the bot was tagged in) and `comment_post`
/// (a comment under a post). The id is derived from the triggering object id
/// (`feedback.id`) and NAMESPACED by kind so a post id and a comment id can
/// never collide. We must NOT mix in `notif.date`, because OpenVK returns the
/// SAME mention multiple times with DIFFERENT dates.
///
/// Returns `None` for notification kinds we don't act on.
fn notification_dedup_id(notif: &Notification) -> Option<u64> {
    let ntype = notif.notification_type.as_deref().unwrap_or("");
    let is_mention = ntype == "mention";
    let is_comment_post = ntype == "comment_post";
    if !is_mention && !is_comment_post {
        return None;
    }

    let post_obj = if is_mention {
        notif.feedback.as_ref()
    } else {
        notif.parent.as_ref()
    }?;
    let post_id = post_obj.id? as u64;

    let trigger_id = notif
        .feedback
        .as_ref()
        .and_then(|f| f.id)
        .unwrap_or(post_id as i64) as u64;

    Some(if is_mention {
        trigger_id.wrapping_mul(10).wrapping_add(1)
    } else {
        trigger_id.wrapping_mul(10).wrapping_add(2)
    })
}

#[tokio::main]
async fn main() -> Result<()> {

    // Load configuration from environment
    let config = Config::from_env()?;

    // Initialize logger
    logger::init_logger_dual(&config.log_file_path, &config.log_level, config.log_console)?;

    info!("НейроРаб bot starting...");
    info!("Bot mode: {:?}", config.bot_mode);
    info!("Configuration loaded successfully");

    // Initialize database
    let db = Arc::new(Database::new(&config.database_path)?);
    info!("Database initialized");

    // Initialize API clients
    let openvk_client = Arc::new(OpenVKClient::new(
        config.openvk_api_url.clone(),
        config.openvk_api_token.clone(),
        config.openvk_hide_online_activity,
    ));
    let claude_ai = ClaudeAI::new(
        config.claude_api_url.clone(),
        config.claude_api_key.clone(),
        config.claude_model.clone(),
    );
    let search_engine = DuckDuckGoSearch::new(config.duckduckgo_api_url.clone());
    let scraper = WebScraper::new(config.max_page_size_mb, config.request_timeout_secs);
    let context_manager = ContextManager::new(db.clone(), config.context_memory_size);

    info!("All clients initialized");

    // Choose bot mode and run
    match config.bot_mode {
        BotMode::Wall => {
            info!("Running in Wall polling mode");
            run_wall_polling(
                openvk_client.as_ref(),
                &claude_ai,
                &search_engine,
                &scraper,
                &context_manager,
                &db,
                &config,
            )
            .await?
        }
        BotMode::Global => {
            info!("Running in Global LongPoll mode");
            run_longpoll_listener(
                openvk_client.clone(),
                &claude_ai,
                &search_engine,
                &scraper,
                &context_manager,
                &db,
                &config,
            )
            .await?
        }
    }

    Ok(())
}

/// Run bot in wall polling mode (legacy mode)
async fn run_wall_polling(
    openvk_client: &OpenVKClient,
    claude_ai: &ClaudeAI,
    search_engine: &DuckDuckGoSearch,
    scraper: &WebScraper,
    context_manager: &ContextManager,
    db: &Arc<Database>,
    config: &Config,
) -> Result<()> {
    info!("Starting wall polling mode");
    let mut last_post_offset = 0u32;
    let polling_interval = Duration::from_secs(config.polling_interval_secs);

    loop {
        match run_poll_iteration(
            openvk_client,
            claude_ai,
            search_engine,
            scraper,
            context_manager,
            db,
            config,
            &mut last_post_offset,
        )
        .await
        {
            Ok(_) => {}
            Err(e) => {
                error!("Error during poll iteration: {}", e);
            }
        }

        sleep(polling_interval).await;
    }
}

/// Run bot in global mode.
///
/// We run a single manual loop (NOT tokio::spawn, because `Database` is not
/// `Send`) that interleaves two pollers:
///   1. LongPoll history polling (messages.getLongPollHistory) for personal
///      messages — handled by `handle_longpoll_notification`.
///   2. Notifications polling (notifications.get) for @mentions and comments on
///      posts — handled by `handle_notification`.
///
/// Both share the same OpenVK client and run sequentially, so there are no
/// thread-safety issues with the SQLite-backed Database.
async fn run_longpoll_listener(
    openvk_client: Arc<OpenVKClient>,
    claude_ai: &ClaudeAI,
    search_engine: &DuckDuckGoSearch,
    scraper: &WebScraper,
    context_manager: &ContextManager,
    db: &Arc<Database>,
    config: &Config,
) -> Result<()> {
    info!("Starting global mode (LongPoll DMs + Notifications mentions/comments)");

    // Initialize the LongPoll server data (gives us the starting `ts`).
    let mut server_data = openvk_client.messages_get_longpoll_server().await?;

    // SEED already-existing notifications as "processed" on startup.
    //
    // Instead of comparing a notification's timestamp against the bot's start
    // time (FRAGILE — any clock skew between the OpenVK server and the host
    // would make valid NEW notifications look "old" and be dropped, which was
    // the main cause of the bot missing replies), we determine "freshness"
    // purely from our LOCAL database. On startup we mark every notification that
    // ALREADY exists in the feed as processed, so the bot won't flood-reply to a
    // backlog after a (re)start, but WILL answer anything that arrives later.
    match openvk_client.notifications_get(50).await {
        Ok(existing) => {
            let mut seeded = 0u32;
            for notif in &existing {
                // Seed the notification's own dedup id (post-level mention).
                if let Some(dedup_id) = notification_dedup_id(notif) {
                    if !db.is_comment_processed(dedup_id).unwrap_or(false) {
                        let _ = db.add_processed_comment(&db::ProcessedComment {
                            comment_id: dedup_id,
                            wall_owner_id: 0,
                            comment_text: "[seeded on startup]".to_string(),
                            bot_response: String::new(),
                            processed_at: chrono::Utc::now().to_rfc3339(),
                        });
                        seeded += 1;
                    }
                }

                // ALSO seed every existing comment under the mentioned post as
                // processed. Real OpenVK mention notifications point at the POST
                // (not the comment), so freshness for comment-mentions is keyed
                // by the comment id (id*10+2). Without seeding these, a restart
                // would make the bot re-answer every old comment mention.
                let ntype = notif.notification_type.as_deref().unwrap_or("");
                let post_src = if ntype == "mention" {
                    notif.feedback.as_ref()
                } else if ntype == "comment_post" {
                    notif.parent.as_ref()
                } else {
                    None
                };
                if let Some(p) = post_src {
                    if let Some(pid) = p.id {
                        let owner = p.to_id.or(p.owner_id).unwrap_or(0);
                        if let Ok(comments) = openvk_client
                            .wall_get_comments(owner, pid as u64, 100, 0)
                            .await
                        {
                            for c in &comments {
                                let cdedup = c.id.wrapping_mul(10).wrapping_add(2);
                                if !db.is_comment_processed(cdedup).unwrap_or(false) {
                                    let _ = db.add_processed_comment(&db::ProcessedComment {
                                        comment_id: cdedup,
                                        wall_owner_id: owner,
                                        comment_text: "[seeded comment on startup]".to_string(),
                                        bot_response: String::new(),
                                        processed_at: chrono::Utc::now().to_rfc3339(),
                                    });
                                    seeded += 1;
                                }
                            }
                        }
                    }
                }
            }
            info!("Seeded {} existing notifications/comments as processed on startup", seeded);
            // Clear the unread badge in the web UI for the startup backlog.
            let _ = openvk_client.notifications_mark_as_viewed().await;
        }
        Err(e) => error!("Failed to seed notifications on startup: {}", e),
    }

    // Throttle the notifications poll so we don't hit the API every cycle.
    let notif_interval = Duration::from_secs(config.notif_poll_interval_secs);
    let mut last_notif_poll = std::time::Instant::now()
        .checked_sub(notif_interval)
        .unwrap_or_else(std::time::Instant::now);

    loop {
        // --- 1. Poll personal messages via LongPoll history ---
        match openvk_client.longpoll_listen(&mut server_data).await {
            Ok(notifications) => {
                for notification in notifications {
                    if let Err(e) = handle_longpoll_notification(
                        notification,
                        openvk_client.as_ref(),
                        claude_ai,
                        search_engine,
                        scraper,
                        context_manager,
                        db,
                        config,
                    )
                    .await
                    {
                        error!("Error handling LongPoll DM: {}", e);
                    }
                }
            }
            Err(e) => {
                // Distinguish a transient NETWORK timeout from a real API error.
                //
                // A timeout just means openvk.xyz was slow to respond this once.
                // We must NOT re-fetch the LongPoll server in that case, because
                // re-fetching resets `ts` and can DROP events that arrived between
                // the old and new ts (i.e. silently lose messages). Instead we
                // keep the SAME `ts` and simply retry — getLongPollHistory will
                // return those events on the next poll. Only a genuine API /
                // protocol error (e.g. an expired ts) warrants re-fetching.
                let msg = e.to_string().to_lowercase();
                let is_transient = msg.contains("timed out")
                    || msg.contains("timeout")
                    || msg.contains("error sending request")
                    || msg.contains("connection")
                    || msg.contains("connect")
                    || msg.contains("body");

                if is_transient {
                    warn!("LongPoll transient network error: {} — retrying with same ts in 2s", e);
                    sleep(Duration::from_secs(2)).await;
                } else {
                    error!("LongPoll API error: {} — re-fetching server, retrying in 3s", e);
                    sleep(Duration::from_secs(3)).await;
                    if let Ok(sd) = openvk_client.messages_get_longpoll_server().await {
                        server_data = sd;
                    }
                }
            }
        }

        // --- 2. Poll notifications (mentions + comments) periodically ---
        if last_notif_poll.elapsed() >= notif_interval {
            last_notif_poll = std::time::Instant::now();
            match openvk_client.notifications_get(20).await {
                Ok(notifs) => {
                    let mut handled_any = false;
                    for notif in &notifs {
                        match handle_notification(
                            notif,
                            openvk_client.as_ref(),
                            claude_ai,
                            search_engine,
                            scraper,
                            context_manager,
                            db,
                            config,
                        )
                        .await
                        {
                            Ok(did_handle) => handled_any |= did_handle,
                            Err(e) => error!("Error handling notification: {}", e),
                        }
                    }

                    // After processing a fresh batch, clear the web UI unread
                    // badge. The authoritative "already handled" state lives in
                    // our DB, so this is purely cosmetic and best-effort.
                    if handled_any {
                        let _ = openvk_client.notifications_mark_as_viewed().await;
                    }
                }
                Err(e) => error!("notifications.get error: {}", e),
            }
        }
    }
}


/// Handle a single notification from notifications.get.
///
/// IMPORTANT (learned from real openvk.xyz data): when the bot is tagged, OpenVK
/// sends `type:"mention"` whose `feedback` is the POST (parent:null, with NO
/// comment id) — even when the actual tag was written INSIDE a comment. So we
/// cannot learn the triggering comment from the notification alone. Instead we
/// resolve the post, load its whole comment thread, and:
///   1. reply IN-THREAD (wall.createComment + reply_to_comment) to every comment
///      that mentions the bot and hasn't been handled yet, and
///   2. if the POST text itself mentions the bot, post one top-level comment.
///
/// Context (post text + full thread) is seeded before generating each reply so
/// the bot understands the whole conversation, not just the trigger line.
///
/// Returns `Ok(true)` if the bot posted at least one reply.
async fn handle_notification(
    notif: &Notification,
    openvk_client: &OpenVKClient,
    claude_ai: &ClaudeAI,
    search_engine: &DuckDuckGoSearch,
    scraper: &WebScraper,
    context_manager: &ContextManager,
    db: &Arc<Database>,
    config: &Config,
) -> Result<bool> {
    let ntype = notif.notification_type.as_deref().unwrap_or("");
    let is_mention = ntype == "mention";
    let is_comment_post = ntype == "comment_post";
    if !is_mention && !is_comment_post {
        return Ok(false);
    }

    // Resolve the POST (id + owner + text). For a "mention" the post is in
    // `feedback`; for a "comment_post" it's in `parent`.
    let post_src = if is_mention {
        notif.feedback.as_ref()
    } else {
        notif.parent.as_ref()
    };
    let post_src = match post_src {
        Some(p) => p,
        None => {
            info!("Notification ({}) has no post object, skipping", ntype);
            return Ok(false);
        }
    };
    let post_id = match post_src.id {
        Some(id) => id as u64,
        None => {
            info!("Notification ({}) post has no id, skipping", ntype);
            return Ok(false);
        }
    };
    let owner_id = post_src.to_id.or(post_src.owner_id).unwrap_or(0);
    let post_text = post_src.text.clone().unwrap_or_default();

    // --- Seed the POST text into context (best-effort). ---
    if !post_text.trim().is_empty() {
        context_manager
            .add_comment_context(
                owner_id,
                post_id,
                owner_id.unsigned_abs(),
                "Пост".to_string(),
                post_text.clone(),
            )
            .await
            .ok();
    } else if let Ok(posts) = openvk_client.wall_get_by_id(owner_id, post_id).await {
        for p in &posts {
            if !p.text.trim().is_empty() {
                context_manager
                    .add_comment_context(
                        owner_id,
                        post_id,
                        p.from_id.unwrap_or(owner_id).unsigned_abs(),
                        "Пост".to_string(),
                        p.text.clone(),
                    )
                    .await
                    .ok();
            }
        }
    }

    // --- Load the whole comment thread and seed it into context. ---
    let comments = openvk_client
        .wall_get_comments(owner_id, post_id, 100, 0)
        .await
        .unwrap_or_default();
    for c in &comments {
        context_manager
            .add_comment_context(
                owner_id,
                post_id,
                c.author_id,
                c.author_id.to_string(),
                c.text.clone(),
            )
            .await
            .ok();
    }

    let mut handled = false;

    // --- 1. Reply IN-THREAD to each comment that mentions the bot. ---
    for c in &comments {
        // Never reply to ourselves.
        if c.author_id == config.openvk_bot_id {
            continue;
        }
        if !MentionDetector::contains_mention_for_bot(
            &c.text,
            &config.bot_mention_prefix,
            config.openvk_bot_id,
            &config.bot_mention_aliases,
        ) {
            continue;
        }
        // Stable dedup key per comment (namespaced with +2 like comment_post).
        let dedup_id = c.id.wrapping_mul(10).wrapping_add(2);
        if db.is_comment_processed(dedup_id)? {
            continue;
        }

        info!(
            "📨 Mention in comment {} on post {}_{} | text=\"{}\"",
            c.id, owner_id, post_id, truncate_str(&c.text, 60)
        );

        let trigger_comment = Comment {
            id: dedup_id,
            owner_id,
            author_id: c.author_id,
            text: c.text.clone(),
            reply_to_comment: None,
            reply_to_user: None,
            date: c.date,
            likes_count: None,
            likes: None,
            attachments: None,
            can_edit: None,
            can_delete: None,
        };

        match generate_bot_response(
            &trigger_comment,
            claude_ai,
            search_engine,
            scraper,
            context_manager,
            config,
            owner_id,
            post_id,
        )
        .await
        {
            Ok(response) => {
                match openvk_client
                    .wall_create_comment_reply(owner_id, post_id, c.id, response.clone())
                    .await
                {
                    Ok(cid) => {
                        info!("✅ Replied (comment {}) to comment {} on post {}_{}", cid, c.id, owner_id, post_id);
                        context_manager
                            .add_comment_context(
                                owner_id,
                                post_id,
                                config.openvk_bot_id,
                                config.bot_name.clone(),
                                response.clone(),
                            )
                            .await
                            .ok();
                        db.add_processed_comment(&db::ProcessedComment {
                            comment_id: dedup_id,
                            wall_owner_id: owner_id,
                            comment_text: c.text.clone(),
                            bot_response: response,
                            processed_at: chrono::Utc::now().to_rfc3339(),
                        })?;
                        handled = true;
                    }
                    Err(e) => error!("Failed to post reply to comment {}: {}", c.id, e),
                }
            }
            Err(e) => error!("Failed to generate reply for comment {}: {}", c.id, e),
        }
    }

    // --- 2. If the POST text itself mentions the bot, reply top-level once. ---
    if MentionDetector::contains_mention_for_bot(
        &post_text,
        &config.bot_mention_prefix,
        config.openvk_bot_id,
        &config.bot_mention_aliases,
    ) {
        let dedup_id = post_id.wrapping_mul(10).wrapping_add(1);
        if !db.is_comment_processed(dedup_id)? {
            info!("📨 Mention in POST {}_{}", owner_id, post_id);
            let trigger_comment = Comment {
                id: dedup_id,
                owner_id,
                author_id: owner_id.unsigned_abs(),
                text: if post_text.is_empty() {
                    format!("{} ?", config.bot_mention_prefix)
                } else {
                    post_text.clone()
                },
                reply_to_comment: None,
                reply_to_user: None,
                date: notif.date.unwrap_or(0),
                likes_count: None,
                likes: None,
                attachments: None,
                can_edit: None,
                can_delete: None,
            };

            match generate_bot_response(
                &trigger_comment,
                claude_ai,
                search_engine,
                scraper,
                context_manager,
                config,
                owner_id,
                post_id,
            )
            .await
            {
                Ok(response) => {
                    match openvk_client
                        .wall_create_comment(owner_id, post_id, response.clone())
                        .await
                    {
                        Ok(cid) => {
                            info!("✅ Posted top-level comment {} on post {}_{}", cid, owner_id, post_id);
                            context_manager
                                .add_comment_context(
                                    owner_id,
                                    post_id,
                                    config.openvk_bot_id,
                                    config.bot_name.clone(),
                                    response.clone(),
                                )
                                .await
                                .ok();
                            db.add_processed_comment(&db::ProcessedComment {
                                comment_id: dedup_id,
                                wall_owner_id: owner_id,
                                comment_text: post_text.clone(),
                                bot_response: response,
                                processed_at: chrono::Utc::now().to_rfc3339(),
                            })?;
                            handled = true;
                        }
                        Err(e) => error!("Failed to post top-level comment: {}", e),
                    }
                }
                Err(e) => error!("Failed to generate response for post mention: {}", e),
            }
        }
    }

    Ok(handled)
}


/// Handle a single LongPoll notification
/// OpenVK LongPoll only supports event type 4 (NewMessage from direct messages)
async fn handle_longpoll_notification(
    notification: ParsedNotification,
    openvk_client: &OpenVKClient,
    claude_ai: &ClaudeAI,
    search_engine: &DuckDuckGoSearch,
    scraper: &WebScraper,
    context_manager: &ContextManager,
    db: &Arc<Database>,
    config: &Config,
) -> Result<()> {
    info!(
        "🔔 Handling LongPoll notification: event_type={:?}, from_user={}, message_id={}, text=\"{}\"",
        notification.event_type, notification.peer_id, notification.message_id, 
        truncate_str(&notification.text, 100)
    );


    // Check if message has already been processed
    if db.is_comment_processed(notification.message_id)? {
        info!("Message {} already processed, skipping", notification.message_id);
        return Ok(());
    }

    // For direct messages in LongPoll, we simply respond to the user directly
    // This is a personal message, so we respond back in a personal message
    
    info!(
        "Processing personal message from user {} - content: {}",
        notification.peer_id, notification.text
    );

    // Use peer_id as the conversation thread id so each DM dialog has its own
    // isolated context (previously hardcoded 0, mixing ALL users together).
    let dm_thread_id = notification.peer_id.unsigned_abs();

    // Add the user's message to this conversation's context
    context_manager
        .add_comment_context(
            notification.peer_id,           // wall_owner = the peer
            dm_thread_id,                    // thread per-user (was 0 for everyone!)
            notification.peer_id.unsigned_abs(),
            notification.peer_id.to_string(),
            notification.text.clone(),
        )
        .await?;


    // Create a dummy Comment struct for generate_bot_response
    let dummy_comment = Comment {
        id: notification.message_id,
        owner_id: notification.peer_id,
        author_id: notification.peer_id as u64,
        text: notification.text.clone(),
        reply_to_comment: None,
        reply_to_user: None,
        date: notification.timestamp,
        likes_count: None,
        likes: None,
        attachments: None,
        can_edit: None,
        can_delete: None,
    };

    // Generate AI response (use dm_thread_id so context is per-conversation)
    match generate_bot_response(
        &dummy_comment,
        claude_ai,
        search_engine,
        scraper,
        context_manager,
        config,
        notification.peer_id,
        dm_thread_id,
    )
    .await
    {
        Ok(mut response) => {
            info!(
                "💬 Generated response to personal message from user {}: {}",
                notification.peer_id, truncate_str(&response, 100)
            );

            // Limit response length for OpenVK API (max message length). Char-safe.
            if response.chars().count() > 10000 {
                response = format!("{}...", truncate_str(&response, 9997));
            }


            // Send response as personal message back to the user
            match openvk_client.messages_send(notification.peer_id, response.clone()).await {
                Ok(sent_message_id) => {
                    info!(
                        "✅ Successfully sent DM response to user {} with message ID: {}",
                        notification.peer_id, sent_message_id
                    );

                    // Save the bot's OWN reply into the conversation context so it
                    // remembers what it said (gives real dialog memory).
                    context_manager
                        .add_comment_context(
                            notification.peer_id,
                            dm_thread_id,
                            config.openvk_bot_id,
                            config.bot_name.clone(),
                            response.clone(),
                        )
                        .await
                        .ok();

                    // Store processed message in database
                    db.add_processed_comment(&db::ProcessedComment {
                        comment_id: notification.message_id,
                        wall_owner_id: notification.peer_id,
                        comment_text: notification.text.clone(),
                        bot_response: response,
                        processed_at: chrono::Utc::now().to_rfc3339(),
                    })?;
                }

                Err(e) => {
                    error!("Failed to send personal message response to user {}: {}", notification.peer_id, e);
                }
            }
        }
        Err(e) => {
            error!("Failed to generate bot response: {}", e);
        }
    }

    Ok(())
}

async fn run_poll_iteration(
    openvk_client: &OpenVKClient,
    claude_ai: &ClaudeAI,
    search_engine: &DuckDuckGoSearch,
    scraper: &WebScraper,
    context_manager: &ContextManager,
    db: &Arc<Database>,
    config: &Config,
    offset: &mut u32,
) -> Result<()> {
    info!("Starting poll iteration");

    // Fetch recent posts from wall
    let posts = openvk_client
        .wall_get(config.openvk_bot_id as i64, 10, *offset)
        .await?;

    if posts.is_empty() {
        info!("No posts found in this iteration");
        return Ok(());
    }

    // Process each post for comments
    for post in posts {
        process_post(
            openvk_client,
            claude_ai,
            search_engine,
            scraper,
            context_manager,
            db,
            config,
            post.owner_id,
            post.id,
        )
        .await?;
    }

    // Update offset for next iteration
    *offset += 10;

    Ok(())
}

async fn process_post(
    openvk_client: &OpenVKClient,
    claude_ai: &ClaudeAI,
    search_engine: &DuckDuckGoSearch,
    scraper: &WebScraper,
    context_manager: &ContextManager,
    db: &Arc<Database>,
    config: &Config,
    owner_id: i64,
    post_id: u64,
) -> Result<()> {
    info!("Processing post {}_{}", owner_id, post_id);

    // Fetch all comments for this post
    let comments = openvk_client
        .wall_get_comments(owner_id, post_id, 100, 0)
        .await?;

    for comment in comments {
        // Check if comment has already been processed
        if db.is_comment_processed(comment.id)? {
            continue;
        }

        // Check if bot is mentioned (textual prefix OR real [id..] tag).
        if !MentionDetector::contains_mention_for_bot(
            &comment.text,
            &config.bot_mention_prefix,
            config.openvk_bot_id,
            &config.bot_mention_aliases,
        ) {
            continue;
        }

        info!(
            "Processing comment {} - bot mention detected",
            comment.id
        );

        // Add comment to context
        context_manager
            .add_comment_context(
                owner_id,
                post_id,
                comment.author_id,
                comment.author_id.to_string(),
                comment.text.clone(),
            )
            .await?;

        // Generate AI response
        match generate_bot_response(
            &comment,
            claude_ai,
            search_engine,
            scraper,
            context_manager,
            config,
            owner_id,
            post_id,
        )
        .await
        {
            Ok(response) => {
                // Post the response
                if let Err(e) = openvk_client
                    .wall_create_comment_reply(owner_id, post_id, comment.id, response.clone())
                    .await
                {
                    error!("Failed to post bot response: {}", e);
                } else {
                    info!("Successfully posted bot response to comment {}", comment.id);

                    // Store processed comment in database
                    db.add_processed_comment(&db::ProcessedComment {
                        comment_id: comment.id,
                        wall_owner_id: owner_id,
                        comment_text: comment.text,
                        bot_response: response,
                        processed_at: chrono::Utc::now().to_rfc3339(),
                    })?;
                }
            }
            Err(e) => {
                error!("Failed to generate bot response: {}", e);
            }
        }
    }

    Ok(())
}

async fn generate_bot_response(
    comment: &openvk::Comment,
    claude_ai: &ClaudeAI,
    search_engine: &DuckDuckGoSearch,
    scraper: &WebScraper,
    context_manager: &ContextManager,
    config: &Config,
    _owner_id: i64,
    post_id: u64,
) -> Result<String> {
    // Get thread context for this post
    let context = context_manager
        .get_limited_thread_context(post_id, config.context_memory_size)
        .await?;

    // Clean the comment text from mention
    let clean_text = comment
        .text
        .replace(&config.bot_mention_prefix, "")
        .trim()
        .to_string();

    // Check if user is asking for fact checking or web search
    let needs_web_search = clean_text.contains("проверить") || clean_text.contains("найти")
        || clean_text.contains("check") || clean_text.contains("search")
        || clean_text.contains("look");

    let mut final_response = if needs_web_search {
        // Perform web search
        match search_engine.search(&clean_text).await {
            Ok(results) => {
                if !results.is_empty() {
                    let search_context = results
                        .iter()
                        .take(3)
                        .map(|r| format!("{}: {}", r.title, r.snippet))
                        .collect::<Vec<_>>()
                        .join("\n\n");

                    let ai_prompt = format!(
                        "Основываясь на следующих результатах поиска, ответь на вопрос: {}\n\nРезультаты:\n{}",
                        clean_text, search_context
                    );

                    claude_ai.generate_response_with_context(ai_prompt, context).await?
                } else {
                    claude_ai
                        .generate_response_with_context(clean_text.clone(), context)
                        .await?
                }
            }
            Err(e) => {
                error!("Web search failed: {}", e);
                claude_ai
                    .generate_response_with_context(clean_text.clone(), context)
                    .await?
            }
        }
    } else {
        // Regular response
        claude_ai
            .generate_response_with_context(clean_text.clone(), context)
            .await?
    };

    // Check for URLs in the comment and scrape if needed
    extract_and_analyze_urls(&clean_text, claude_ai, scraper, &mut final_response).await?;

    // Limit response length for OpenVK API (max comment length). Char-safe.
    if final_response.chars().count() > 10000 {
        final_response = format!("{}...", truncate_str(&final_response, 9997));
    }

    Ok(final_response)

}

async fn extract_and_analyze_urls(
    text: &str,
    claude_ai: &ClaudeAI,
    scraper: &WebScraper,
    response: &mut String,
) -> Result<()> {
    // Simple URL extraction using regex
    let url_regex = regex::Regex::new(r"https?://[^\s]+").ok();

    if let Some(regex) = url_regex {
        for url_match in regex.find_iter(text) {
            let url = url_match.as_str();

            // Try to fetch and analyze the page content
            match scraper.fetch_content(url).await {
                Ok(content) => {
                    // Limit content for analysis (take first 5000 chars,
                    // char-safe so multi-byte UTF-8 never panics).
                    let limited_content = truncate_str(&content.text, 5000);

                    if let Ok(analysis) = claude_ai.analyze_web_content(url, &limited_content).await {

                        response.push_str("\n\n📄 Анализ ссылки:\n");
                        response.push_str(&analysis);
                    }
                }
                Err(e) => {
                    error!("Failed to fetch and analyze URL {}: {}", url, e);
                }
            }
        }
    }

    Ok(())
}
