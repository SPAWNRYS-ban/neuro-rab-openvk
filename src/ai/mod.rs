 pub mod claude;

pub use claude::ClaudeAI;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Rich(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

impl Serialize for Message {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("role", &self.role)?;
        
        match &self.content {
            MessageContent::Text(text) => {
                map.serialize_entry("content", text)?;
            }
            MessageContent::Rich(blocks) => {
                map.serialize_entry("content", &blocks)?;
            }
        }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for Message {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct MessageHelper {
            role: String,
            content: serde_json::Value,
        }
        
        let helper = MessageHelper::deserialize(deserializer)?;
        let content = match helper.content {
            serde_json::Value::String(s) => MessageContent::Text(s),
            serde_json::Value::Array(arr) => {
                let blocks: Vec<ContentBlock> = arr
                    .into_iter()
                    .filter_map(|v| serde_json::from_value(v).ok())
                    .collect();
                MessageContent::Rich(blocks)
            }
            _ => MessageContent::Text(String::new()),
        };
        
        Ok(Message {
            role: helper.role,
            content,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIResponse {
    pub content: String,
    pub tokens_used: Option<u32>,
}
