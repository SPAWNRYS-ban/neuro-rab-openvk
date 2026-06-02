use super::SearchResult;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DuckDuckGoResponse {
    pub results: Option<Vec<DuckDuckGoResult>>,
    pub abstract_url: Option<String>,
    pub abstract_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DuckDuckGoResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
}

pub struct DuckDuckGoSearch {
    client: Client,
    api_url: String,
}

impl DuckDuckGoSearch {
    pub fn new(api_url: String) -> Self {
        DuckDuckGoSearch {
            client: Client::new(),
            api_url,
        }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        debug!("Searching DuckDuckGo for: {}", query);

        let response = self
            .client
            .get(&self.api_url)
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("no_redirect", "1"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            error!("DuckDuckGo search failed: {}", response.status());
            return Err(anyhow!("DuckDuckGo search failed: {}", response.status()));
        }

        let ddg_response: DuckDuckGoResponse = response.json().await?;

        let mut results = Vec::new();

        // Add abstract result if available
        if let (Some(url), Some(text)) = (ddg_response.abstract_url, ddg_response.abstract_text) {
            results.push(SearchResult {
                title: "Direct Answer".to_string(),
                url,
                snippet: text,
            });
        }

        // Add regular results
        if let Some(search_results) = ddg_response.results {
            for result in search_results {
                results.push(SearchResult {
                    title: result.title,
                    url: result.url,
                    snippet: result.snippet.unwrap_or_default(),
                });
            }
        }

        info!("Found {} search results", results.len());

        Ok(results)
    }

    pub async fn search_fact_check(&self, query: &str) -> Result<Vec<SearchResult>> {
        debug!("Fact-checking search for: {}", query);

        let fact_check_query = format!("{} fact check verification", query);
        self.search(&fact_check_query).await
    }
}
