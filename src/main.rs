mod ai;
mod config;
mod context;
mod db;
mod logger;
mod openvk;
mod web;

use ai::ClaudeAI;
use anyhow::Result;
use config::Config;
use context::{ContextManager, MentionDetector};
use db::Database;
use log::{error, info};
use openvk::OpenVKClient;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use web::{DuckDuckGoSearch, WebScraper};

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration from environment
    let config = Config::from_env()?;

    // Initialize logger
    logger::init_logger_dual(&config.log_file_path, &config.log_level, config.log_console)?;

    info!("НейроРаб bot starting...");
    info!("Configuration loaded successfully");

    // Initialize database
    let db = Arc::new(Database::new(&config.database_path)?);
    info!("Database initialized");

    // Initialize API clients
    let openvk_client = OpenVKClient::new(config.openvk_api_url.clone(), config.openvk_api_token.clone());
    let claude_ai = ClaudeAI::new(
        config.claude_api_url.clone(),
        config.claude_api_key.clone(),
        config.claude_model.clone(),
    );
    let search_engine = DuckDuckGoSearch::new(config.duckduckgo_api_url.clone());
    let scraper = WebScraper::new(config.max_page_size_mb, config.request_timeout_secs);
    let context_manager = ContextManager::new(db.clone(), config.context_memory_size);

    info!("All clients initialized");

    // Bot state variables
    let mut last_post_offset = 0u32;
    let polling_interval = Duration::from_secs(config.polling_interval_secs);

    loop {
        match run_poll_iteration(
            &openvk_client,
            &claude_ai,
            &search_engine,
            &scraper,
            &context_manager,
            &db,
            &config,
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

    // Limit response length for OpenVK API (max comment length)
    if final_response.len() > 10000 {
        final_response.truncate(9997);
        final_response.push_str("...");
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
                    // Limit content for analysis (take first 5000 chars)
                    let limited_content = if content.text.len() > 5000 {
                        &content.text[..5000]
                    } else {
                        &content.text
                    };

                    if let Ok(analysis) = claude_ai.analyze_web_content(url, limited_content).await {
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
