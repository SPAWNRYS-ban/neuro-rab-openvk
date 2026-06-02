use super::Message;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

// tokenator.cloud uses an OpenAI-compatible API format
#[derive(Debug, Clone, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Debug, Clone, Deserialize)]
struct ClaudeMessageResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
struct Choice {
    message: ChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChoiceMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}


pub struct ClaudeAI {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
}

impl ClaudeAI {
    pub fn new(api_url: String, api_key: String, model: String) -> Self {
        ClaudeAI {
            client: Client::new(),
            api_url,
            api_key,
            model,
        }
    }

    pub async fn chat(&self, messages: Vec<Message>, system: Option<String>) -> Result<String> {
        // tokenator.cloud uses OpenAI-compatible endpoint
        let url = format!("{}/chat/completions", self.api_url);

        debug!("Sending request to Claude API (OpenAI-compatible): {}", url);

        // In OpenAI format, the system prompt is passed as the first message with role="system"
        let mut full_messages = Vec::new();
        if let Some(system_prompt) = system {
            full_messages.push(Message {
                role: "system".to_string(),
                content: system_prompt,
            });
        }
        full_messages.extend(messages);

        let request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            messages: full_messages,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!("Claude API error: {}", error_text);
            return Err(anyhow!("Claude API error: {}", error_text));
        }

        let claude_response: ClaudeMessageResponse = response.json().await?;

        let content = claude_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| anyhow!("No choices in Claude response"))?;

        info!("Received response from Claude API");

        Ok(content)
    }


    pub async fn generate_response(
        &self,
        user_message: String,
        context: Option<Vec<String>>,
    ) -> Result<String> {
        let mut system_prompt = String::from(
            "You are НейроРаб (NeuroSlave), an AI assistant on the OpenVK social network. \
            Your task is to:\n\
            1. Respond to comments when mentioned (@НейроРаб)\n\
            2. Match the 'vibe' and tone of the conversation\n\
            3. Provide helpful and contextual answers\n\
            4. Be concise but informative\n\
            5. Respect the user's request for fact-checking when needed\
            \n\nRespond in the same language as the user."
        );

        if let Some(ctx) = context {
            system_prompt.push_str("\n\nContext from previous messages:\n");
            for (idx, msg) in ctx.iter().enumerate() {
                system_prompt.push_str(&format!("{}. {}\n", idx + 1, msg));
            }
        }

        let messages = vec![Message {
            role: "user".to_string(),
            content: user_message,
        }];

        self.chat(messages, Some(system_prompt)).await
    }

    pub async fn generate_response_with_context(
        &self,
        user_message: String,
        context_thread: Vec<(String, String)>, // (author, message) pairs
    ) -> Result<String> {
        let mut system_prompt = String::from(
            "You are НейроРаб (NeuroSlave), an AI assistant on the OpenVK social network. \
            Your task is to:\n\
            1. Respond to comments when mentioned (@НейроРаб)\n\
            2. Match the 'vibe' and tone of the conversation\n\
            3. Provide helpful and contextual answers\n\
            4. Be concise but informative\n\
            5. Respond in the same language as the user\
            \n\nYou are currently in a discussion thread with the following context:\n"
        );

        for (author, message) in context_thread {
            system_prompt.push_str(&format!("{}: {}\n", author, message));
        }

        let messages = vec![Message {
            role: "user".to_string(),
            content: user_message,
        }];

        self.chat(messages, Some(system_prompt)).await
    }

    pub async fn fact_check(&self, statement: String) -> Result<String> {
        let system_prompt = "You are a fact-checking assistant. \
        Analyze the given statement and provide a brief assessment of its accuracy. \
        If you need to search the web for information, indicate what you would search for. \
        Be clear and concise in your response.";

        let messages = vec![Message {
            role: "user".to_string(),
            content: format!("Please fact-check the following statement: {}", statement),
        }];

        self.chat(messages, Some(system_prompt.to_string()))
            .await
    }

    pub async fn analyze_web_content(&self, url: &str, content: &str) -> Result<String> {
        let system_prompt = format!(
            "You are a web content analyzer. Analyze the following content from {} \
            and provide a summary or relevant information based on the context of the conversation.",
            url
        );

        let messages = vec![Message {
            role: "user".to_string(),
            content: format!(
                "Please analyze this web content and summarize the key points:\n\n{}",
                content
            ),
        }];

        self.chat(messages, Some(system_prompt)).await
    }
}
