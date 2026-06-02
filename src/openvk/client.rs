use super::{
    Comment, Post, WallCreateCommentResponse, WallGetCommentsResponse, WallGetResponse,

    LongPollServerResponse, LongPollServerData, EventType, ParsedNotification, 
    NotificationsGetResponse, Notification,
};
use anyhow::{anyhow, Result};
use reqwest::Client;
use tracing::{debug, error, info, warn};
use std::time::{Duration, Instant};
use tokio::time::sleep;


pub struct OpenVKClient {
    client: Client,
    api_url: String,
    api_token: String,
    hide_online_activity: bool,
}

impl OpenVKClient {
    pub fn new(api_url: String, api_token: String, hide_online_activity: u32) -> Self {
        // IMPORTANT: build the client WITH an explicit request timeout.
        // `Client::new()` has NO timeout by default, so if the OpenVK server
        // ever fails to respond to a request (e.g. wall.getComments on a
        // virtual mention post), the request hangs FOREVER. Because the bot
        // runs a single interleaved loop, one hung request freezes the whole
        // bot — it stops answering DMs AND notifications. A 30s timeout makes
        // such a request fail fast so the loop can recover.
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());

        OpenVKClient {
            client,
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
                ("need_pts", "1".to_string()),
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

    /// Listen for new events using messages.getLongPollHistory.
    ///
    /// IMPORTANT: We intentionally do NOT use the `a_check` LongPoll endpoint
    /// (the /nim<id> server URL). Testing against openvk.xyz showed that the
    /// a_check endpoint is UNRELIABLE: the server only holds the first couple of
    /// connections for the full `wait` period, after which it returns an empty
    /// array `[]` almost instantly AND silently drops any events that arrive while
    /// the bot is busy (e.g. generating an AI reply). This caused the bot to
    /// answer only the very first message and then go deaf.
    ///
    /// `messages.getLongPollHistory(ts)` is reliable: it returns ALL events that
    /// occurred since `ts`, even ones the bot "missed". The event format inside
    /// the `history` array is identical to a_check updates:
    /// [4, messageId, flags, peer_id, timestamp, text, ...]
    ///
    /// We poll this method on a fixed interval and advance `ts` past the newest
    /// event we've seen so we never receive duplicates.
    pub async fn longpoll_listen(
        &self,
        server_data: &mut LongPollServerData,
    ) -> Result<Vec<ParsedNotification>> {
        const POLL_INTERVAL: Duration = Duration::from_secs(3);

        let url = format!("{}/method/messages.getLongPollHistory", self.api_url);

        debug!("Polling getLongPollHistory at ts={}", server_data.ts);

        let request_start = Instant::now();
        let response = self
            .client
            .get(&url)
            .query(&[
                ("access_token", self.api_token.clone()),
                ("ts", server_data.ts.to_string()),
                ("lp_version", "3".to_string()),
                ("msgs_limit", "200".to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            warn!("getLongPollHistory returned status: {}", response.status());
            return Err(anyhow!("getLongPollHistory error: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;

        // Check for API error
        if let Some(error) = data.get("error") {
            error!("getLongPollHistory API error: {}", error);
            return Err(anyhow!("getLongPollHistory API error: {}", error));
        }

        // Parse the `history` array from the response
        let mut notifications = Vec::new();
        let mut failed_parses = 0;
        let mut max_event_ts = server_data.ts;

        if let Some(history) = data
            .get("response")
            .and_then(|r| r.get("history"))
            .and_then(|h| h.as_array())
        {
            if !history.is_empty() {
                info!("🔔 getLongPollHistory returned {} events", history.len());
            }

            for (idx, event) in history.iter().enumerate() {
                // Track the newest event timestamp (index 4) so we can advance ts
                if let Some(arr) = event.as_array() {
                    if let Some(ev_ts) = arr.get(4).and_then(|v| v.as_u64()) {
                        if ev_ts > max_event_ts {
                            max_event_ts = ev_ts;
                        }
                    }
                }

                match self.parse_longpoll_event(event) {
                    Ok(notification) => {
                        info!(
                            "✅ Event {}: Parsed - EventType={:?}, MessageID={}, PeerID={}, Text=\"{}\"",
                            idx, notification.event_type, notification.message_id,
                            notification.peer_id, notification.text.chars().take(50).collect::<String>()

                        );
                        notifications.push(notification);
                    }
                    Err(e) => {
                        warn!(
                            "⚠️ Event {}: Failed to parse - {} | Raw: {}",
                            idx, e, serde_json::to_string(event).unwrap_or_default()
                        );
                        failed_parses += 1;
                    }
                }
            }
        } else {
            debug!("No history field in getLongPollHistory response");
        }

        // Advance ts past the newest event so we don't receive duplicates.
        // +1 because the server returns events with timestamp >= ts.
        if max_event_ts > server_data.ts {
            server_data.ts = max_event_ts + 1;
            debug!("Advanced ts to {}", server_data.ts);
        }

        if !notifications.is_empty() {
            info!("✨ Processed {} new events (failed: {})", notifications.len(), failed_parses);
        }

        // Throttle: getLongPollHistory returns instantly, so pace our polling
        // to avoid hammering the server.
        let elapsed = request_start.elapsed();
        if elapsed < POLL_INTERVAL {
            sleep(POLL_INTERVAL - elapsed).await;
        }

        Ok(notifications)
    }



    /// Send a personal message to a user
    pub async fn messages_send(&self, user_id: i64, message: String) -> Result<u64> {
        let url = format!("{}/method/messages.send", self.api_url);

        debug!("Sending personal message to user {} from {}", user_id, url);

        let mut query_params = vec![
            ("user_id", user_id.to_string()),
            ("message", message.clone()),
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

        // Parse response for message_id
        let response_text = response.text().await?;
        debug!("messages.send response: {}", response_text);

        let json_response: serde_json::Value = serde_json::from_str(&response_text)?;

        // Check for error
        if let Some(error) = json_response.get("error") {
            if let Some(error_msg) = error.get("error_msg") {
                error!("OpenVK API error sending message: {}", error_msg);
                return Err(anyhow!("OpenVK API error: {}", error_msg));
            }
        }

        // Extract message_id from response
        if let Some(message_id) = json_response.get("response").and_then(|r| r.as_u64()) {
            info!("Successfully sent personal message with ID: {}", message_id);
            Ok(message_id)
        } else {
            error!("No message_id in response: {}", response_text);
            Err(anyhow!("No message_id in messages.send response"))
        }
    }

    /// Get notifications (mentions, comments, likes, etc.)
    pub async fn notifications_get(&self, count: u32) -> Result<Vec<Notification>> {
        let url = format!("{}/method/notifications.get", self.api_url);

        debug!("Fetching notifications from {}", url);

        let mut query_params = vec![
            ("count", count.to_string()),
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

        let notif_response: NotificationsGetResponse = response.json().await?;

        if let Some(error) = notif_response.error {
            error!("OpenVK API error getting notifications: {}", error.error_msg);
            return Err(anyhow!("OpenVK API error: {}", error.error_msg));
        }

        let data = notif_response
            .response
            .ok_or_else(|| anyhow!("No response from notifications.get"))?;

        info!("Fetched {} notifications", data.items.len());
        Ok(data.items)
    }

    /// Parse a single LongPoll event to get notification details
    /// OpenVK LongPoll event type 4 (NewMessage) format:
    /// [4, messageId, spam_flag, peer_id, timestamp, text, info, attachments, random_id, conversation_id, edited]
    fn parse_longpoll_event(&self, event: &serde_json::Value) -> Result<ParsedNotification> {
        let event_array = event
            .as_array()
            .ok_or_else(|| anyhow!("Event is not an array"))?;

        if event_array.len() < 5 {
            return Err(anyhow!("Event array too short (need at least 5 elements for type 4)"));
        }

        let event_code = event_array[0]
            .as_u64()
            .ok_or_else(|| anyhow!("Event code is not u64"))? as u32;

        // Log ALL event codes for debugging (this is critical!)
        tracing::debug!("🔍 Received raw event code: {} (full event: {})", event_code, serde_json::to_string(event).unwrap_or_default());

        let event_type =
            EventType::from_code(event_code).ok_or_else(|| anyhow!("Unknown event code: {} | OpenVK LongPoll only supports event type 4 (NewMessage)", event_code))?;

        // For event type 4 (NewMessage):
        // [4, messageId, spam_flag, peer_id, timestamp, text, info, attachments, random_id, conversation_id, edited]
        let message_id = event_array[1]
            .as_u64()
            .ok_or_else(|| anyhow!("Message ID is not u64"))?;

        // spam_flag at index 2 (we don't need it)
        
        let peer_id = event_array[3]
            .as_i64()
            .ok_or_else(|| anyhow!("Peer ID is not i64"))?;

        let timestamp = event_array[4]
            .as_u64()
            .ok_or_else(|| anyhow!("Timestamp is not u64"))?;

        // Get text if available (at index 5)
        let text = if event_array.len() > 5 {
            event_array[5]
                .as_str()
                .unwrap_or("New message")
                .to_string()
        } else {
            "New message".to_string()
        };

        Ok(ParsedNotification {
            event_type,
            message_id,
            peer_id,
            text,
            timestamp,
        })
    }
}
