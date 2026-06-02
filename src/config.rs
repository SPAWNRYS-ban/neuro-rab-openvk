use anyhow::Result;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub openvk_api_url: String,
    pub openvk_api_token: String,
    pub openvk_bot_id: u64,
    pub claude_api_url: String,
    pub claude_api_key: String,
    pub claude_model: String,
    pub duckduckgo_api_url: String,
    pub database_path: String,
    pub polling_interval_secs: u64,
    pub log_level: String,
    pub log_file_path: String,
    pub log_console: bool,
    pub max_page_size_mb: u64,
    pub request_timeout_secs: u64,
    pub bot_mention_prefix: String,
    pub bot_name: String,
    pub context_memory_size: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        Ok(Config {
            openvk_api_url: env::var("OPENVK_API_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            openvk_api_token: env::var("OPENVK_API_TOKEN")?,
            openvk_bot_id: env::var("OPENVK_BOT_ID")
                .unwrap_or_else(|_| "1".to_string())
                .parse()?,
            claude_api_url: env::var("CLAUDE_API_URL")
                .unwrap_or_else(|_| "https://api.tokenator.cloud/v1".to_string()),
            claude_api_key: env::var("CLAUDE_API_KEY")?,
            claude_model: env::var("CLAUDE_MODEL")
                .unwrap_or_else(|_| "claude-3-5-haiku-20241022".to_string()),
            duckduckgo_api_url: env::var("DUCKDUCKGO_API_URL")
                .unwrap_or_else(|_| "https://api.duckduckgo.com".to_string()),
            database_path: env::var("DATABASE_PATH")
                .unwrap_or_else(|_| "./bot_cache.db".to_string()),
            polling_interval_secs: env::var("POLLING_INTERVAL_SECS")
                .unwrap_or_else(|_| "6".to_string())
                .parse()?,
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            log_file_path: env::var("LOG_FILE_PATH")
                .unwrap_or_else(|_| "./logs/bot.log".to_string()),
            log_console: env::var("LOG_CONSOLE")
                .unwrap_or_else(|_| "true".to_string())
                .parse()?,
            max_page_size_mb: env::var("MAX_PAGE_SIZE_MB")
                .unwrap_or_else(|_| "10".to_string())
                .parse()?,
            request_timeout_secs: env::var("REQUEST_TIMEOUT_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()?,
            bot_mention_prefix: env::var("BOT_MENTION_PREFIX")
                .unwrap_or_else(|_| "@НейроРаб".to_string()),
            bot_name: env::var("BOT_NAME").unwrap_or_else(|_| "НейроРаб".to_string()),
            context_memory_size: env::var("CONTEXT_MEMORY_SIZE")
                .unwrap_or_else(|_| "10".to_string())
                .parse()?,
        })
    }
}
