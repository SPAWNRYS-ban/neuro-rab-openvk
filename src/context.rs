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
        // Trim ONLY this thread's history (per-conversation), so other DM
        // dialogs / post threads keep their own memory intact.
        self.cleanup_old_context(thread_id).await?;

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

    /// Clean up old context entries for ONE thread/conversation.
    async fn cleanup_old_context(&self, thread_id: u64) -> Result<()> {
        self.db
            .clear_old_context_for_thread(thread_id, self.memory_size)?;
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

    /// Detect whether the bot is mentioned, considering BOTH:
    ///   1. The textual prefix (e.g. "@НейроРаб" / "НейроРаб"), AND
    ///   2. The REAL OpenVK mention tag the platform inserts when you tag a
    ///      user: `[id{bot_id}|Display Name]` or `[id{bot_id}]`.
    ///
    /// Previously the bot only matched the textual prefix, so a real tag (which
    /// does NOT contain the literal "@НейроРаб" string) was never recognized and
    /// the bot stayed silent. This method fixes that.
    pub fn contains_mention_for_bot(
        text: &str,
        mention: &str,
        bot_id: u64,
        aliases: &[String],
    ) -> bool {
        // 1. Textual prefix match (case-insensitive on the un-@ name).
        let name = mention.replace('@', "");
        let lower = text.to_lowercase();
        if text.contains(mention)
            || (!name.is_empty() && lower.contains(&name.to_lowercase()))
        {
            return true;
        }

        // 1b. Any configured alias / shortname (case-insensitive), e.g.
        // "neuroslave" / "@neuroslave".
        for a in aliases {
            let a = a.trim().trim_start_matches('@');
            if !a.is_empty() && lower.contains(&a.to_lowercase()) {
                return true;
            }
        }

        // 2. Real OpenVK mention tag: [id{bot_id}|...] or [id{bot_id}]
        let tag_prefix = format!("[id{}", bot_id);
        if let Some(pos) = text.find(&tag_prefix) {
            // Make sure the character right after the id is a delimiter
            // (`|` or `]`), so `[id4134` doesn't match `[id41343...]`.
            let after = &text[pos + tag_prefix.len()..];
            if after.starts_with('|') || after.starts_with(']') {
                return true;
            }
        }

        // OpenVK also supports club mentions as [club{id}|...]; include for
        // completeness in case the bot is run as a group.
        let club_prefix = format!("[club{}", bot_id);
        if let Some(pos) = text.find(&club_prefix) {
            let after = &text[pos + club_prefix.len()..];
            if after.starts_with('|') || after.starts_with(']') {
                return true;
            }
        }

        false
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
