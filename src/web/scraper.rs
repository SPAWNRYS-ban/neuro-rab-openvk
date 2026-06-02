use super::WebContent;
use anyhow::{anyhow, Result};
use reqwest::Client;
use scraper::Html;
use std::time::Duration;
use tracing::{debug, error, info};

pub struct WebScraper {
    client: Client,
    max_page_size_mb: u64,
    timeout_secs: u64,
}

impl WebScraper {
    pub fn new(max_page_size_mb: u64, timeout_secs: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());

        WebScraper {
            client,
            max_page_size_mb,
            timeout_secs,
        }
    }

    pub async fn fetch_content(&self, url: &str) -> Result<WebContent> {
        debug!("Fetching content from: {}", url);

        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            error!("Failed to fetch {}: {}", url, response.status());
            return Err(anyhow!("Failed to fetch URL: {}", response.status()));
        }

        // Check content length
        if let Some(content_length) = response.content_length() {
            let max_bytes = self.max_page_size_mb * 1024 * 1024;
            if content_length > max_bytes {
                error!(
                    "Content size {} exceeds maximum {} MB",
                    content_length, self.max_page_size_mb
                );
                return Err(anyhow!(
                    "Content exceeds maximum size of {} MB",
                    self.max_page_size_mb
                ));
            }
        }

        let body = response.bytes().await?;

        // Check retrieved content size
        let max_bytes = self.max_page_size_mb * 1024 * 1024;
        if body.len() > max_bytes as usize {
            error!(
                "Retrieved content size {} exceeds maximum {} MB",
                body.len(),
                self.max_page_size_mb
            );
            return Err(anyhow!(
                "Content exceeds maximum size of {} MB",
                self.max_page_size_mb
            ));
        }

        let html_content = String::from_utf8_lossy(&body).to_string();
        let text = self.extract_text(&html_content)?;

        let title = self.extract_title(&html_content);

        info!("Successfully scraped content from: {}", url);

        Ok(WebContent {
            url: url.to_string(),
            title,
            text,
        })
    }

    fn extract_text(&self, html: &str) -> Result<String> {
        let document = Html::parse_document(html);

        // Remove script and style tags
        let mut text = String::new();

        for selector_str in &["p", "div", "span", "h1", "h2", "h3", "h4", "h5", "h6", "li"] {
            if let Ok(selector) = scraper::Selector::parse(selector_str) {
                for element in document.select(&selector) {
                    let inner_html = element.inner_html();
                    // Remove HTML tags
                    let cleaned = self.strip_html_tags(&inner_html);
                    if !cleaned.trim().is_empty() {
                        text.push_str(&cleaned);
                        text.push('\n');
                    }
                }
            }
        }

        if text.is_empty() {
            // Fallback: extract all text content
            text = document
                .root_element()
                .inner_html()
                .chars()
                .map(|c| if c == '<' { ' ' } else { c })
                .collect::<String>();

            // Remove remaining HTML-like patterns
            text = self.strip_html_tags(&text);
        }

        Ok(text.trim().to_string())
    }

    fn extract_title(&self, html: &str) -> Option<String> {
        let document = Html::parse_document(html);

        if let Ok(selector) = scraper::Selector::parse("title") {
            if let Some(title_element) = document.select(&selector).next() {
                return Some(title_element.inner_html());
            }
        }

        // Fallback: try to get h1
        if let Ok(selector) = scraper::Selector::parse("h1") {
            if let Some(h1_element) = document.select(&selector).next() {
                return Some(h1_element.inner_html());
            }
        }

        None
    }

    fn strip_html_tags(&self, html: &str) -> String {
        let regex = regex::Regex::new(r"<[^>]*>").unwrap();
        let text = regex.replace_all(html, " ");

        // Decode HTML entities
        html_escape::decode_html_entities(&text).to_string()
    }

    pub async fn fetch_links(&self, url: &str) -> Result<Vec<String>> {
        debug!("Fetching links from: {}", url);

        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to fetch URL: {}", response.status()));
        }

        let body = response.bytes().await?;
        let html_content = String::from_utf8_lossy(&body);
        let document = Html::parse_document(&html_content);

        let mut links = Vec::new();

        if let Ok(selector) = scraper::Selector::parse("a") {
            for element in document.select(&selector) {
                if let Some(href) = element.value().attr("href") {
                    // Try to convert relative URLs to absolute
                    if let Ok(absolute_url) = url::Url::parse(url).and_then(|base| base.join(href))
                    {
                        links.push(absolute_url.to_string());
                    }
                }
            }
        }

        info!("Found {} links on: {}", links.len(), url);

        Ok(links)
    }
}
