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
    pub owner_id: i64,
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
