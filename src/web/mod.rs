pub mod scraper;
pub mod search;

pub use scraper::WebScraper;
pub use search::DuckDuckGoSearch;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebContent {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
}
