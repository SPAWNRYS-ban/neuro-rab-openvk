use super::{
    Comment, Post, WallCreateCommentResponse, WallGetCommentsResponse, WallGetResponse,
    LongPollServerResponse, LongPollServerData, EventType, ParsedNotification,
};
use anyhow::{anyhow, Result};
use reqwest::Client;
use tracing::{debug, error, info, warn};
use std::time::Duration;

pub struct OpenVKClient {
    client: Client,
    api_url: String,
    api_token: String,
    hide_online_activity: bool,
}

impl OpenVKClient {
    pub fn new(api_url: String, api_token: String, hide_online_activity: u32) -> Self {
        OpenVKClient {
            client: Client::new(),
            api_url,
            api_token,
            hide_online_activity: hide_online_activity != 0,
        }
    }

    /// Get wall posts for a specified owner
    pub async fn wall_get(
        &self,
        owner_id: i64,
        count: u32,
        offset: u32,
    ) -> Result<Vec<Post>> {
        let url = format!("{}/method/wall.get", self.api_url);

        debug!("Fetching wall posts from {}", url);

        let mut query_params = vec![
            ("owner_id", owner_id.to_string()),
            ("count", count.to_string()),
            ("offset", offset.to_string()),
            ("access_token", self.api_token.clone()),
        ];

        let hide_online = if self.hide_online_activity {
            Some(("forGodSakePleaseDoNotReportAboutMyOnlineActivity", "1".to_string()))
        } else {
            None
        };

        if let Some(param) = hide_online {
            query_params.push(param);
        }

        let response = self
            .client
            .get(&url)
            .query(&query_params)
            .send()
            .await?;

        let wall_response: WallGetResponse = response.json().await?;

        if let Some(error) = wall_response.error {
            error!("OpenVK API error: {}", error.error_msg);
            return Err(anyhow!("OpenVK API error: {}", error.error_msg));
        }

        let data = wall_response
            .response
            .ok_or_else(|| anyhow!("No response from wall.get"))?;

        info!("Fetched {} wall posts", data.items.len());
        Ok(data.items)
    }

    /// Get comments for a specific wall post
    pub async fn wall_get_comments(
        &self,
        owner_id: i64,
        post_id: u64,
        count: u32,
        offset: u32,
    ) -> Result<Vec<Comment>> {
        let url = format!("{}/method/wall.getComments", self.api_url);

        debug!(
            "Fetching comments for post {}_{} from {}",
            owner_id, post_id, url
        );

        let mut query_params = vec![
            ("owner_id", owner_id.to_string()),
            ("post_id", post_id.to_string()),
            ("count", count.to_string()),
            ("offset", offset.to_string()),
            ("access_token", self.api_token.clone()),
            ("extended", "1".to_string()),
        ];

        let hide_online = if self.hide_online_activity {
            Some(("forGodSakePleaseDoNotReportAboutMyOnlineActivity", "1".to_string()))
        } else {
            None
        };

        if let Some(param) = hide_online {
            query_params.push(param);
        }

        let response = self
            .client
            .get(&url)
            .query(&query_params)
            .send()
            .await?;

        let comments_response: WallGetCommentsResponse = response.json().await?;

        if let Some(error) = comments_response.error {
            error!("OpenVK API error: {}", error.error_msg);
            return Err(anyhow!("OpenVK API error: {}", error.error_msg));
        }

        let data = comments_response
            .response
            .ok_or_else(|| anyhow!("No response from wall.getComments"))?;

        info!("Fetched {} comments", data.items.len());
        Ok(data.items)
    }

    /// Create a comment on a wall post
    pub async fn wall_create_comment(
        &self,
        owner_id: i64,
        post_id: u64,
        text: String,
    ) -> Result<u64> {
        let url = format!("{}/method/wall.createComment", self.api_url);

        debug!("Creating comment on post {}_{}", owner_id, post_id);

        let mut query_params = vec![
            ("owner_id", owner_id.to_string()),
            ("post_id", post_id.to_string()),
            ("message", text.clone()),
            ("access_token", self.api_token.clone()),
        ];

        let hide_online = if self.hide_online_activity {
            Some(("forGodSakePleaseDoNotReportAboutMyOnlineActivity", "1".to_string()))
        } else {
            None
        };

        if let Some(param) = hide_online {
            query_params.push(param);
        }

        let response = self
            .client
            .post(&url)
            .query(&query_params)
            .send()
            .await?;

        let create_response: WallCreateCommentResponse = response.json().await?;

        if let Some(error) = create_response.error {
            error!("OpenVK API error: {}", error.error_msg);
            return Err(anyhow!("OpenVK API error: {}", error.error_msg));
        }

        let data = create_response
            .response
            .ok_or_else(|| anyhow!("No response from wall.createComment"))?;

        info!("Successfully created comment with ID: {}", data.comment_id);
        Ok(data.comment_id)
    }

    /// Create a reply to a specific comment
    pub async fn wall_create_comment_reply(
        &self,
        owner_id: i64,
        post_id: u64,
        reply_to_comment: u64,
        text: String,
    ) -> Result<u64> {
        let url = format!("{}/method/wall.createComment", self.api_url);

        debug!(
            "Creating reply to comment {} on post {}_{}\n",
            reply_to_comment, owner_id, post_id
        );

        let mut query_params = vec![
            ("owner_id", owner_id.to_string()),
            ("post_id", post_id.to_string()),
            ("reply_to_comment", reply_to_comment.to_string()),
            ("message", text.clone()),
            ("access_token", self.api_token.clone()),
        ];

        let hide_online = if self.hide_online_activity {
            Some(("forGodSakePleaseDoNotReportAboutMyOnlineActivity", "1".to_string()))
        } else {
            None
        };

        if let Some(param) = hide_online {
            query_params.push(param);
        }

        let response = self
            .client
            .post(&url)
            .query(&query_params)
            .send()
            .await?;

        let create_response: WallCreateCommentResponse = response.json().await?;

        if let Some(error) = create_response.error {
            error!("OpenVK API error: {}", error.error_msg);
            return Err(anyhow!("OpenVK API error: {}", error.error_msg));
        }

        let data = create_response
            .response
            .ok_or_else(|| anyhow!("No response from wall.createComment"))?;

        info!("Successfully created reply with ID: {}", data.comment_id);
        Ok(data.comment_id)
    }

    /// Get LongPoll server information
    pub async fn messages_get_longpoll_server(&self) -> Result<LongPollServerData> {
        let url = format!("{}/method/messages.getLongPollServer", self.api_url);

        debug!("Getting LongPoll server from {}", url);

        let response = self
            .client
            .get(&url)
            .query(&[
                ("access_token", self.api_token.clone()),
                ("need_pts", "0".to_string()),
                ("lp_version", "3".to_string()),
            ])
            .send()
            .await?;

        let lp_response: LongPollServerResponse = response.json().await?;

        if let Some(error) = lp_response.error {
            error!("OpenVK API error getting LongPoll server: {}", error.error_msg);
            return Err(anyhow!(
                "OpenVK API error: {}",
                error.error_msg
            ));
        }

        let data = lp_response
            .response
            .ok_or_else(|| anyhow!("No response from messages.getLongPollServer"))?;

        info!(
            "Got LongPoll server: {} with key: {}",
            data.server,
            &data.key[..data.key.len().min(10)]
        );
        Ok(data)
    }

    /// Listen to LongPoll events from server
    /// Returns Vec of parsed notifications when new events arrive
    pub async fn longpoll_listen(
        &self,
        server_data: &mut LongPollServerData,
    ) -> Result<Vec<ParsedNotification>> {
        let url = format!("{}?act=a_check&key={}&ts={}&wait=25", server_data.server, server_data.key, server_data.ts);

        debug!("Connecting to LongPoll server for updates...");

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            warn!(
                "LongPoll server returned status: {}",
                response.status()
            );
            return Err(anyhow!("LongPoll server error: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;

        // Update timestamp for next request
        if let Some(new_ts) = data.get("ts").and_then(|v| v.as_u64()) {
            server_data.ts = new_ts;
        }

        // Parse updates
        let mut notifications = Vec::new();
        if let Some(updates) = data.get("updates").and_then(|v| v.as_array()) {
            for update in updates {
                if let Ok(notification) = self.parse_longpoll_event(update) {
                    notifications.push(notification);
                }
            }
        }

        if !notifications.is_empty() {
            info!("Received {} notifications from LongPoll", notifications.len());
        }

        Ok(notifications)
    }

    /// Parse a single LongPoll event to get notification details
    fn parse_longpoll_event(&self, event: &serde_json::Value) -> Result<ParsedNotification> {
        // LongPoll event format: [event_code, object_id, user_id, etc...]
        let event_array = event
            .as_array()
            .ok_or_else(|| anyhow!("Event is not an array"))?;

        if event_array.len() < 3 {
            return Err(anyhow!("Event array too short"));
        }

        let event_code = event_array[0]
            .as_u64()
            .ok_or_else(|| anyhow!("Event code is not u64"))? as u32;

        let event_type =
            EventType::from_code(event_code).ok_or_else(|| anyhow!("Unknown event code: {}", event_code))?;

        let object_id = event_array[1]
            .as_i64()
            .ok_or_else(|| anyhow!("Object ID is not i64"))?;
        let user_id = event_array[2]
            .as_i64()
            .ok_or_else(|| anyhow!("User ID is not i64"))?;

        // For events, we need to extract post_id and comment_id
        // The format varies, so we parse additional fields
        let post_id = if event_array.len() > 3 {
            event_array[3].as_u64().unwrap_or(0)
        } else {
            0
        };

        let comment_id = object_id.unsigned_abs();
        let wall_owner_id = if object_id > 0 { object_id } else { -object_id };

        // Get text if available
        let text = if event_array.len() > 4 {
            event_array[4]
                .as_str()
                .unwrap_or("New notification")
                .to_string()
        } else {
            "New notification".to_string()
        };

        Ok(ParsedNotification {
            event_type,
            wall_owner_id,
            post_id,
            comment_id,
            from_id: user_id,
            text,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
        })
    }
}
