use super::{
    Comment, Post, WallCreateCommentResponse, WallGetCommentsResponse, WallGetResponse,
};
use anyhow::{anyhow, Result};
use reqwest::Client;
use tracing::{debug, error, info};

pub struct OpenVKClient {
    client: Client,
    api_url: String,
    api_token: String,
}

impl OpenVKClient {
    pub fn new(api_url: String, api_token: String) -> Self {
        OpenVKClient {
            client: Client::new(),
            api_url,
            api_token,
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

        let response = self
            .client
            .get(&url)
            .query(&[
                ("owner_id", owner_id.to_string()),
                ("count", count.to_string()),
                ("offset", offset.to_string()),
                ("access_token", self.api_token.clone()),
            ])
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

        let response = self
            .client
            .get(&url)
            .query(&[
                ("owner_id", owner_id.to_string()),
                ("post_id", post_id.to_string()),
                ("count", count.to_string()),
                ("offset", offset.to_string()),
                ("access_token", self.api_token.clone()),
                ("extended", "1".to_string()),
            ])
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

        let response = self
            .client
            .post(&url)
            .query(&[
                ("owner_id", owner_id.to_string()),
                ("post_id", post_id.to_string()),
                ("text", text.clone()),
                ("access_token", self.api_token.clone()),
            ])
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

        let response = self
            .client
            .post(&url)
            .query(&[
                ("owner_id", owner_id.to_string()),
                ("post_id", post_id.to_string()),
                ("reply_to_comment", reply_to_comment.to_string()),
                ("text", text.clone()),
                ("access_token", self.api_token.clone()),
            ])
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
}
