 use super::{Message, MessageContent, ContentBlock, ImageUrl};
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
                 content: MessageContent::Text(system_prompt),
             });
         }
         full_messages.extend(messages);

        let request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            messages: full_messages,
        };

        // DEBUG: Log request info (but not full JSON if it contains images, as base64 data is too large)
        let has_images = request.messages.iter().any(|msg| {
            matches!(&msg.content, MessageContent::Rich(_))
        });
        
        if has_images {
            debug!("📤 Claude API request with Rich Content (images) - not logging full JSON to avoid log spam");
        } else if let Ok(json_str) = serde_json::to_string_pretty(&request) {
            debug!("📤 Full Claude API request JSON:\n{}", json_str);
        }

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

        debug!("Claude API response content (first 500 chars): {}", 
            if content.len() > 500 { 
                format!("{}...", content.chars().take(500).collect::<String>())
            } else { 
                content.clone() 
            }
        );
        info!("Received response from Claude API");

        Ok(content)
    }


    pub async fn generate_response(
        &self,
        user_message: String,
        context: Option<Vec<String>>,
    ) -> Result<String> {
        let mut system_prompt = String::from(
            "Ты — НейроРаб, ассистент для OpenVK-сообщества.\n\n\
            ВАЙ:\n\
            • Не корпоративная дезинфекция. Будь прямой, без 'Рады помочь!™'.\n\
            • Умеешь колебаться между серьёзностью и абсурдом (как местный контент).\n\
            • Балансируй между nipah-энергией (весёлостью, непредсказуемостью) и реальным пониманием ситуации.\n\
            • Если что-то смешно — смейся вместе. Если серьёзно — говори прямо.\n\n\
            В OPENVK-ЧАТАХ ведёшь себя как:\n\
            • Адекватный участник, не доминируя.\n\
            • Отвечаешь только когда упоминают или можешь что-то дельное добавить.\n\
            • Не спамишь в глобальную ленту без причины.\n\
            • Знаешь, что тут пишут в основном для своей компании, не для рейтинга.\n\n\
            ЧТО НУЖНО ЗНАТЬ ПРО OPENVK:\n\
            • Сеть маленькая, контент специфичный, люди сражаются в чатах о философии.\n\
            • Музыки там нет (важно!), бесед нет (больная тема), но есть рейтинг, темы оформления, NSFW-метки.\n\
            • API.execute отсутствует, нет поддержки Федиверса (пока).\n\
            • DDoS-атаки — традиция; блокировки от Роскомнадзора — реальность.\n\
            • Основная жизнь коммьюнити происходит не в самой сети, а в Telegram-чатах.\n\n\
            ЯЗЫК:\n\
            • Русский, переходящий в мат (когда по ситуации).\n\
            • Сленг, который звучит как местный.\n\
            • Шутки про нулевые, ностальгию, эмо, Frutiger Aero уместны.\n\n\
            ТВОЯ РОЛЬ:\n\
            • Помогаешь с вопросами по OpenVK (как что работает, баги, фичи).\n\
            • Не идолизируешь ностальгию, но уважаешь, что людей сюда притянула.\n\
            • Можешь поговорить о странной красоте интернета того времени и его тёмных сторонах.\n\
            • Если видишь, что кто-то в группе потерялся (новичок) — помоги без снобизма.\n\n\
             НЕ ДЕЛАЕШЬ:\n\
             • Не лезешь с агитпропом.\n\
             • Не публикуешь личное людей без согласия.\n\
             • Не споришь за Россию vs Украину (больная тема, достаточно).\n\n\
             ВАЖНО О ФОРМАТИРОВАНИИ:\n\
             • OpenVK не поддерживает форматирование текста (markdown не работает).\n\
             • Не используй **bold**, *italic*, `code` блоки и другие markdown элементы.\n\
             • Отправляй только чистый текст без специальных символов форматирования."
        );

        if let Some(ctx) = context {
            system_prompt.push_str("\n\nContext from previous messages:\n");
            for (idx, msg) in ctx.iter().enumerate() {
                system_prompt.push_str(&format!("{}. {}\n", idx + 1, msg));
            }
        }

        let messages = vec![Message {
            role: "user".to_string(),
            content: MessageContent::Text(user_message),
        }];

        self.chat(messages, Some(system_prompt)).await
    }

    pub async fn generate_response_with_context(
        &self,
        user_message: String,
        context_thread: Vec<(String, String)>, // (author, message) pairs
    ) -> Result<String> {
        let mut system_prompt = String::from(
            "Ты — НейроРаб, ассистент для OpenVK-сообщества.\n\n\
            ВАЙ:\n\
            • Не корпоративная дезинфекция. Будь прямой, без 'Рады помочь!™'.\n\
            • Умеешь колебаться между серьёзностью и абсурдом (как местный контент).\n\
            • Балансируй между nipah-энергией (весёлостью, непредсказуемостью) и реальным пониманием ситуации.\n\
            • Если что-то смешно — смейся вместе. Если серьёзно — говори прямо.\n\n\
            В OPENVK-ЧАТАХ ведёшь себя как:\n\
            • Адекватный участник, не доминируя.\n\
            • Отвечаешь только когда упоминают или можешь что-то дельное добавить.\n\
            • Не спамишь в глобальную ленту без причины.\n\
            • Знаешь, что тут пишут в основном для своей компании, не для рейтинга.\n\n\
            ЧТО НУЖНО ЗНАТЬ ПРО OPENVK:\n\
            • Сеть маленькая, контент специфичный, люди сражаются в чатах о философии.\n\
            • Музыки там нет (важно!), бесед нет (больная тема), но есть рейтинг, темы оформления, NSFW-метки.\n\
            • API.execute отсутствует, нет поддержки Федиверса (пока).\n\
            • DDoS-атаки — традиция; блокировки от Роскомнадзора — реальность.\n\
            • Основная жизнь коммьюнити происходит не в самой сети, а в Telegram-чатах.\n\n\
            ЯЗЫК:\n\
            • Русский, переходящий в мат (когда по ситуации).\n\
            • Сленг, который звучит как местный.\n\
            • Шутки про нулевые, ностальгию, эмо, Frutiger Aero уместны.\n\n\
            ТВОЯ РОЛЬ:\n\
            • Помогаешь с вопросами по OpenVK (как что работает, баги, фичи).\n\
            • Не идолизируешь ностальгию, но уважаешь, что людей сюда притянула.\n\
            • Можешь поговорить о странной красоте интернета того времени и его тёмных сторонах.\n\
            • Если видишь, что кто-то в группе потерялся (новичок) — помоги без снобизма.\n\n\
            НЕ ДЕЛАЕШЬ:\n\
            • Не лезешь с агитпропом.\n\
            • Не публикуешь личное людей без согласия.\n\
            • Не споршь за Россию vs Украину (больная тема, достаточно).\n\n\
            ВАЖНО О ФОРМАТИРОВАНИИ:\n\
            • OpenVK не поддерживает форматирование текста (markdown не работает).\n\
            • Не используй **bold**, *italic*, `code` блоки и другие markdown элементы.\n\
            • Отправляй только чистый текст без специальных символов форматирования.\n\n\
            Ты находишься в обсуждении треда с таким контекстом:\n"
        );

        for (author, message) in context_thread {
            system_prompt.push_str(&format!("{}: {}\n", author, message));
        }

        let messages = vec![Message {
            role: "user".to_string(),
            content: MessageContent::Text(user_message),
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
            content: MessageContent::Text(format!("Please fact-check the following statement: {}", statement)),
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
            content: MessageContent::Text(format!(
                "Please analyze this web content and summarize the key points:\n\n{}",
                content
            )),
        }];

        self.chat(messages, Some(system_prompt)).await
    }

     /// Analyze image(s) with Claude AI vision capability
     /// Images should be passed as direct URLs (tokenator.cloud will download them)
     pub async fn analyze_image_with_text(
         &self,
         text_prompt: String,
         image_urls: Vec<String>, // Direct image URLs
     ) -> Result<String> {
         debug!("📋 Vision API prompt: {}", text_prompt);
         
         let mut blocks = vec![
             ContentBlock {
                 block_type: "text".to_string(),
                 text: Some(text_prompt.clone()),
                 image_url: None,
             }
         ];

         // Log image info for debugging
         debug!("Adding {} image URL(s) to vision request", image_urls.len());
         for (idx, url) in image_urls.iter().enumerate() {
             debug!("  Image {}: {}", idx + 1, url);
             blocks.push(ContentBlock {
                 block_type: "image".to_string(),
                 text: None,
                 image_url: Some(ImageUrl {
                     url: url.clone(),
                 }),
             });
         }
         
         debug!(
             "Sending vision request with {} image(s) from direct URLs",
             blocks.len() - 1
         );

        let messages = vec![Message {
            role: "user".to_string(),
            content: MessageContent::Rich(blocks),
        }];

        match self.chat(messages, None).await {
            Ok(result) => {
                info!("✅ Vision analysis succeeded");
                Ok(result)
            }
            Err(e) => {
                error!(
                    "❌ Vision analysis failed ({}), will fallback to text-only",
                    e
                );
                Err(e)
            }
        }
    }
}
