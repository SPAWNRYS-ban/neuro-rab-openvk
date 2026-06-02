use crate::db::{ContextEntry, Database};
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

pub struct ContextManager {
    db: std::sync::Arc<Database>,
    memory_size: usize,
}

impl ContextManager {
    pub fn new(db: std::sync::Arc<Database>, memory_size: usize) -> Self {
        ContextManager { db, memory_size }
    }

    /// Add a new context entry from a comment
    pub async fn add_comment_context(
        &self,
        wall_owner_id: i64,
        thread_id: u64,
        author_id: u64,
        author_name: String,
        comment_text: String,
    ) -> Result<()> {
        let entry = ContextEntry {
            id: Uuid::new_v4().to_string(),
            wall_owner_id,
            thread_id,
            author_id,
            content: format!("{}: {}", author_name, comment_text),
            created_at: Utc::now().to_rfc3339(),
        };

        self.db.add_context_entry(&entry)?;
        self.cleanup_old_context().await?;

        Ok(())
    }

    /// Get all context for a specific thread
    pub async fn get_thread_context(&self, thread_id: u64) -> Result<Vec<(String, String)>> {
        let entries = self.db.get_thread_context(thread_id)?;

        Ok(entries
            .into_iter()
            .rev() // Reverse to get chronological order
            .map(|e| (e.author_id.to_string(), e.content))
            .collect())
    }

    /// Get limited context (last N messages) for a specific thread
    pub async fn get_limited_thread_context(
        &self,
        thread_id: u64,
        limit: usize,
    ) -> Result<Vec<(String, String)>> {
        let entries = self.db.get_thread_context(thread_id)?;

        Ok(entries
            .into_iter()
            .rev()
            .take(limit)
            .map(|e| (e.author_id.to_string(), e.content))
            .collect())
    }

    /// Get context as formatted string for AI
    pub async fn get_formatted_context(&self, thread_id: u64) -> Result<String> {
        let entries = self.db.get_thread_context(thread_id)?;

        let mut context = String::new();
        for (idx, entry) in entries.iter().rev().enumerate() {
            context.push_str(&format!("{}. {}\n", idx + 1, entry.content));
        }

        Ok(context)
    }

    /// Clean up old context entries
    async fn cleanup_old_context(&self) -> Result<()> {
        self.db.clear_old_context(self.memory_size)?;
        Ok(())
    }

    /// Clear context for a specific thread
    pub async fn clear_thread_context(&self, _thread_id: u64) -> Result<()> {
        // This would need a database method to implement
        // For now, we rely on clear_old_context which removes old entries
        Ok(())
    }
}

pub struct MentionDetector;

impl MentionDetector {
    pub fn contains_mention(text: &str, mention: &str) -> bool {
        text.contains(mention) || text.contains(&mention.replace("@", ""))
    }

    pub fn extract_mention_context(text: &str, mention: &str) -> String {
        // Find the position of mention
        if let Some(pos) = text.find(mention) {
            // Extract context around the mention (up to 100 chars before and after)
            let start = if pos > 100 { pos - 100 } else { 0 };
            let end = if pos + mention.len() + 100 < text.len() {
                pos + mention.len() + 100
            } else {
                text.len()
            };

            text[start..end].to_string()
        } else {
            text.to_string()
        }
    }

    pub fn is_direct_reply(text: &str, mention: &str) -> bool {
        // Check if mention is at the start or very close to start
        text.trim_start().starts_with(mention)
            || text.trim_start().starts_with(&mention.replace("@", ""))
    }
}
