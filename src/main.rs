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
use log::{error, info};
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

    // Record the bot's startup time (unix epoch). We only respond to
    // notifications (mentions/comments) that arrive AFTER this moment, so the
    // bot doesn't flood-reply to every old mention sitting in the feed when it
    // (re)starts.
    let started_at: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    info!("Notification cutoff: only replying to events after {}", started_at);

    // Initialize the LongPoll server data (gives us the starting `ts`).
    let mut server_data = openvk_client.messages_get_longpoll_server().await?;


    // Throttle the notifications poll so we don't hit the API every cycle.
    let notif_interval = Duration::from_secs(10);
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
                error!("LongPoll error: {} — re-fetching server, retrying in 3s", e);
                sleep(Duration::from_secs(3)).await;
                if let Ok(sd) = openvk_client.messages_get_longpoll_server().await {
                    server_data = sd;
                }
            }
        }

        // --- 2. Poll notifications (mentions + comments) periodically ---
        if last_notif_poll.elapsed() >= notif_interval {
            last_notif_poll = std::time::Instant::now();
            match openvk_client.notifications_get(20).await {
                Ok(notifs) => {
                    for notif in &notifs {
                        if let Err(e) = handle_notification(
                            notif,
                            openvk_client.as_ref(),
                            claude_ai,
                            search_engine,
                            scraper,
                            context_manager,
                            db,
                            config,
                            started_at,
                        )
                        .await
                        {
                            error!("Error handling notification: {}", e);
                        }
                    }

                }
                Err(e) => error!("notifications.get error: {}", e),
            }
        }
    }
}


/// Handle a single notification from notifications.get.
///
/// Real OpenVK format (from Notification::toVkApiStruct / getVkApiInfo):
///   - type = "mention":      feedback = the POST the bot was mentioned in
///                            (NotifObject: id=post_virtual_id, to_id=wall_owner, from_id, text)
///   - type = "comment_post": parent   = the POST, feedback = the COMMENT
///                            (comment NotifObject: id=comment_id, owner_id, text)
///
/// We respond by posting a comment (or reply) on the post, and we load the
/// whole comment thread into context first so the bot knows the conversation.
async fn handle_notification(
    notif: &Notification,
    openvk_client: &OpenVKClient,
    claude_ai: &ClaudeAI,
    search_engine: &DuckDuckGoSearch,
    scraper: &WebScraper,
    context_manager: &ContextManager,
    db: &Arc<Database>,
    config: &Config,
    started_at: u64,
) -> Result<()> {
    let ntype = notif.notification_type.as_deref().unwrap_or("");

    // We only care about mentions in posts and comments on posts.
    let is_mention = ntype == "mention";
    let is_comment_post = ntype == "comment_post";
    if !is_mention && !is_comment_post {
        return Ok(());
    }

    // Skip notifications that happened BEFORE the bot started, so we don't
    // flood-reply to a backlog of old mentions on (re)start.
    let notif_date = notif.date.unwrap_or(0);
    if notif_date < started_at {
        return Ok(());
    }


    // Determine the POST object (where to comment) and the text/author that
    // triggered the notification.
    // - For "mention": the post itself is in `feedback`.
    // - For "comment_post": the post is in `parent`, the comment is in `feedback`.
    let post_obj = if is_mention {
        notif.feedback.as_ref()
    } else {
        notif.parent.as_ref()
    };

    let post_obj = match post_obj {
        Some(p) => p,
        None => {
            info!("Notification ({}) has no post object, skipping", ntype);
            return Ok(());
        }
    };

    // Post id: for a post NotifObject this is `id`; wall owner is `to_id`.
    let post_id = match post_obj.id {
        Some(id) => id as u64,
        None => {
            info!("Notification ({}) post has no id, skipping", ntype);
            return Ok(());
        }
    };
    let owner_id = post_obj.to_id.or(post_obj.owner_id).unwrap_or(0);

    // The text that triggered us + the author who wrote it.
    let trigger = notif.feedback.as_ref();
    let trigger_text = trigger.and_then(|f| f.text.clone()).unwrap_or_default();
    let trigger_author = trigger
        .and_then(|f| f.from_id.or(f.owner_id))
        .unwrap_or(owner_id);

    // For comment_post we only respond if the bot is actually mentioned.
    if is_comment_post
        && !MentionDetector::contains_mention(&trigger_text, &config.bot_mention_prefix)
    {
        return Ok(());
    }

    // Deduplicate using a STABLE key derived from the triggering object id
    // (feedback.id). For a "mention" this is the post id; for "comment_post"
    // this is the comment id. We must NOT mix in `notif.date`, because OpenVK
    // returns the SAME mention multiple times with DIFFERENT dates, which
    // would otherwise make the bot answer the same mention repeatedly.
    //
    // We namespace the two notification kinds so a post id and a comment id
    // can never collide.
    let trigger_id = trigger.and_then(|f| f.id).unwrap_or(post_id as i64) as u64;
    let dedup_id: u64 = if is_mention {
        trigger_id.wrapping_mul(10).wrapping_add(1)
    } else {
        trigger_id.wrapping_mul(10).wrapping_add(2)
    };
    if db.is_comment_processed(dedup_id)? {
        return Ok(());
    }


    info!(
        "📨 Handling {} on post {}_{} | trigger=\"{}\"",
        ntype, owner_id, post_id,
        truncate_str(&trigger_text, 60)
    );


    // Load the whole comment thread into context so the bot knows the conversation.
    if let Ok(comments) = openvk_client.wall_get_comments(owner_id, post_id, 100, 0).await {
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
    }

    // Build a synthetic Comment for the AI generator using the trigger text.
    let trigger_comment = Comment {
        id: dedup_id,
        owner_id,
        author_id: trigger_author.unsigned_abs(),
        text: if trigger_text.is_empty() {
            format!("{} ?", config.bot_mention_prefix)
        } else {
            trigger_text.clone()
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
                    info!("✅ Posted comment {} on post {}_{}", cid, owner_id, post_id);
                    // Record the bot's own answer into context too.
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
                        comment_text: trigger_text,
                        bot_response: response,
                        processed_at: chrono::Utc::now().to_rfc3339(),
                    })?;
                }
                Err(e) => error!("Failed to post comment response: {}", e),
            }
        }
        Err(e) => error!("Failed to generate response for {}: {}", ntype, e),
    }

    Ok(())
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

        // Check if bot is mentioned
        if !MentionDetector::contains_mention(&comment.text, &config.bot_mention_prefix) {
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
