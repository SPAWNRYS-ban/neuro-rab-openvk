use anyhow::Result;
use std::env;

#[derive(Debug, Clone, PartialEq)]
pub enum BotMode {
    Wall,   // Monitor single wall (legacy mode)
    Global, // Monitor global mentions via LongPoll
}

impl BotMode {
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "wall" => BotMode::Wall,
            "global" => BotMode::Global,
            _ => BotMode::Global, // default
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub openvk_api_url: String,
    pub openvk_api_token: String,
    pub openvk_bot_id: u64,
    pub openvk_hide_online_activity: u32,
    pub bot_mode: BotMode,
    pub longpoll_reconnect_interval_secs: u64,
    pub longpoll_max_reconnect_attempts: u32,
    pub longpoll_backoff_multiplier: f64,
    pub longpoll_max_wait_secs: u64,
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
    pub notif_poll_interval_secs: u64,
    pub bot_mention_aliases: Vec<String>,
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
            openvk_hide_online_activity: env::var("OPENVK_HIDE_ONLINE_ACTIVITY")
                .unwrap_or_else(|_| "0".to_string())
                .parse()?,
            bot_mode: BotMode::from_string(
                &env::var("BOT_MODE").unwrap_or_else(|_| "global".to_string()),
            ),
            longpoll_reconnect_interval_secs: env::var("LONGPOLL_RECONNECT_INTERVAL_SECS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()?,
            longpoll_max_reconnect_attempts: env::var("LONGPOLL_MAX_RECONNECT_ATTEMPTS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()?,
            longpoll_backoff_multiplier: env::var("LONGPOLL_BACKOFF_MULTIPLIER")
                .unwrap_or_else(|_| "1.5".to_string())
                .parse()?,
            longpoll_max_wait_secs: env::var("LONGPOLL_MAX_WAIT_SECS")
                .unwrap_or_else(|_| "300".to_string())
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
            notif_poll_interval_secs: env::var("NOTIF_POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()?,
            // Extra textual handles that count as mentioning the bot, in
            // ADDITION to the [id{bot_id}|...] tag and BOT_MENTION_PREFIX.
            // Real OpenVK comments tag the bot by its latin shortname
            // (e.g. "@neuroslave"), which differs from the display prefix.
            bot_mention_aliases: env::var("BOT_MENTION_ALIASES")
                .unwrap_or_else(|_| "neuroslave,НейроРаб".to_string())
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect(),
        })
    }
}
