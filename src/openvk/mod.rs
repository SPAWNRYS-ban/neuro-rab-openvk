pub mod client;

pub use client::OpenVKClient;

use serde::{Deserialize, Serialize};

/// Request parameter for API debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestParam {
    pub key: String,
    pub value: String,
}

/// Comment on a wall post
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    #[serde(default)]
    pub owner_id: i64,
    // OpenVK returns the comment author under `from_id` (NOT `author_id`).
    // Without this alias, deserialization FAILS and the bot silently sees ZERO
    // comments — so it never finds the mention and never replies.
    #[serde(alias = "from_id", default)]
    pub author_id: u64,
    pub text: String,
    pub reply_to_comment: Option<u64>,
    pub reply_to_user: Option<u64>,
    pub date: u64,
    pub likes_count: Option<u64>,
    pub likes: Option<serde_json::Value>,
    pub attachments: Option<Vec<serde_json::Value>>,
    pub can_edit: Option<bool>,
    pub can_delete: Option<bool>,
}

/// Wall post
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: u64,
    pub owner_id: i64,
    pub from_id: Option<i64>,
    pub text: String,
    pub date: u64,
    pub likes: Option<serde_json::Value>,
    pub reposts: Option<serde_json::Value>,
    pub comments: Option<serde_json::Value>,
    pub attachments: Option<Vec<serde_json::Value>>,
    pub can_edit: Option<bool>,
    pub can_delete: Option<bool>,
    pub is_pinned: Option<bool>,
    pub is_archived: Option<bool>,
    pub post_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallGetResponse {
    pub response: Option<WallGetData>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallGetData {
    pub count: u64,
    pub items: Vec<Post>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallGetCommentsResponse {
    pub response: Option<WallGetCommentsData>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallGetCommentsData {
    pub count: u64,
    pub items: Vec<Comment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallCreateCommentResponse {
    pub response: Option<CommentCreateData>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentCreateData {
    pub comment_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error_code: u32,
    pub error_msg: String,
    pub request_params: Option<Vec<RequestParam>>,
}

// LongPoll Support Structures

/// LongPoll Server Information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongPollServerResponse {
    pub response: Option<LongPollServerData>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongPollServerData {
    pub key: String,
    pub server: String,
    pub ts: u64,
}

/// LongPoll Event from server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LongPollEvent {
    // Update type and ts
    Updates(Vec<serde_json::Value>),
}

/// Notification types in LongPoll events
/// OpenVK LongPoll only supports:
/// 4 = new message (message from another user)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventType {
    NewMessage = 4,
}

impl EventType {
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            4 => Some(EventType::NewMessage),
            _ => None,
        }
    }
}

/// Parsed notification from LongPoll event
/// For NewMessage event type 4: [4, msgId, spam_flag, peer, timestamp, text, info, attachments, random_id, conversation_id, edited]
#[derive(Debug, Clone)]
pub struct ParsedNotification {
    pub event_type: EventType,
    pub message_id: u64,
    pub peer_id: i64,           // Who sent the message (from_id)
    pub text: String,
    pub timestamp: u64,
}

// Notifications API Support Structures

/// Notification response from notifications.get
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsGetResponse {
    pub response: Option<NotificationsGetData>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsGetData {
    // NOTE: The real OpenVK notifications.get response does NOT include a
    // top-level `count` field — it only has `items`, `groups`, `profiles`,
    // `last_viewed`. Making this `#[serde(default)]` so deserialization does
    // not fail (which previously caused the bot to silently ignore ALL
    // notifications).
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub items: Vec<Notification>,
}


/// A "feedback" or "parent" object inside a notification.
/// For posts (toNotifApiStruct): { id, to_id, from_id, date, text }
/// For comments (toNotifApiStruct): { id, owner_id, date, text }
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotifObject {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub to_id: Option<i64>,
    #[serde(default)]
    pub from_id: Option<i64>,
    #[serde(default)]
    pub owner_id: Option<i64>,
    #[serde(default)]
    pub date: Option<u64>,
    #[serde(default)]
    pub text: Option<String>,
}

/// Single notification from notifications.get, matching the real OpenVK API.
/// Real format: { "type": "mention"|"comment_post"|..., "date": u64,
///                "parent": NotifObject|null, "feedback": NotifObject|null, "reply": null }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    #[serde(rename = "type", default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub date: Option<u64>,
    #[serde(default)]
    pub parent: Option<NotifObject>,
    #[serde(default)]
    pub feedback: Option<NotifObject>,
}


/// Action codes for Notifications API
/// 0 = like, 1 = repost, 2 = comment, 3 = wall_post, 4 = mention, 5 = moderator, 6 = accepted, 7 = suggested
pub mod notification_action_codes {
    pub const LIKE: u32 = 0;
    pub const REPOST: u32 = 1;
    pub const COMMENT: u32 = 2;
    pub const WALL_POST: u32 = 3;
    pub const MENTION: u32 = 4;
}
