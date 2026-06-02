pub mod client;

pub use client::OpenVKClient;

use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: u64,
    pub owner_id: i64,
    pub text: String,
    pub date: u64,
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
}
