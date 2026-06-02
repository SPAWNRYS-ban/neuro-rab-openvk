use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedComment {
    pub comment_id: u64,
    pub wall_owner_id: i64,
    pub comment_text: String,
    pub bot_response: String,
    pub processed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub id: String,
    pub wall_owner_id: i64,
    pub thread_id: u64,
    pub author_id: u64,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebCache {
    pub id: String,
    pub url: String,
    pub content: String,
    pub created_at: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Database { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS processed_comments (
                comment_id INTEGER PRIMARY KEY,
                wall_owner_id INTEGER NOT NULL,
                comment_text TEXT NOT NULL,
                bot_response TEXT NOT NULL,
                processed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS context_cache (
                id TEXT PRIMARY KEY,
                wall_owner_id INTEGER NOT NULL,
                thread_id INTEGER NOT NULL,
                author_id INTEGER NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(thread_id, author_id)
            );

            CREATE TABLE IF NOT EXISTS web_cache (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_processed_comments_wall ON processed_comments(wall_owner_id);
            CREATE INDEX IF NOT EXISTS idx_context_cache_thread ON context_cache(thread_id);
            CREATE INDEX IF NOT EXISTS idx_web_cache_url ON web_cache(url);
            ",
        )?;
        Ok(())
    }

    // Processed Comments Methods
    pub fn add_processed_comment(&self, comment: &ProcessedComment) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO processed_comments 
             (comment_id, wall_owner_id, comment_text, bot_response, processed_at)
             VALUES (?, ?, ?, ?, ?)",
            params![
                comment.comment_id,
                comment.wall_owner_id,
                &comment.comment_text,
                &comment.bot_response,
                &comment.processed_at
            ],
        )?;
        Ok(())
    }

    pub fn get_processed_comment(&self, comment_id: u64) -> Result<Option<ProcessedComment>> {
        let mut stmt = self.conn.prepare(
            "SELECT comment_id, wall_owner_id, comment_text, bot_response, processed_at 
             FROM processed_comments WHERE comment_id = ?",
        )?;

        let comment = stmt
            .query_row(params![comment_id], |row| {
                Ok(ProcessedComment {
                    comment_id: row.get(0)?,
                    wall_owner_id: row.get(1)?,
                    comment_text: row.get(2)?,
                    bot_response: row.get(3)?,
                    processed_at: row.get(4)?,
                })
            })
            .optional()?;

        Ok(comment)
    }

    pub fn is_comment_processed(&self, comment_id: u64) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM processed_comments WHERE comment_id = ?",
            params![comment_id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    // Context Cache Methods
    pub fn add_context_entry(&self, entry: &ContextEntry) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO context_cache 
             (id, wall_owner_id, thread_id, author_id, content, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                &entry.id,
                entry.wall_owner_id,
                entry.thread_id,
                entry.author_id,
                &entry.content,
                &entry.created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_thread_context(&self, thread_id: u64) -> Result<Vec<ContextEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, wall_owner_id, thread_id, author_id, content, created_at 
             FROM context_cache WHERE thread_id = ? ORDER BY created_at DESC",
        )?;

        let contexts = stmt
            .query_map(params![thread_id], |row| {
                Ok(ContextEntry {
                    id: row.get(0)?,
                    wall_owner_id: row.get(1)?,
                    thread_id: row.get(2)?,
                    author_id: row.get(3)?,
                    content: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(contexts)
    }

    pub fn clear_old_context(&self, keep_count: usize) -> Result<()> {
        self.conn.execute(
            "DELETE FROM context_cache WHERE id NOT IN (
                SELECT id FROM context_cache ORDER BY created_at DESC LIMIT ?
            )",
            params![keep_count],
        )?;
        Ok(())
    }

    // Web Cache Methods
    pub fn add_web_cache(&self, cache: &WebCache) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO web_cache (id, url, content, created_at)
             VALUES (?, ?, ?, ?)",
            params![&cache.id, &cache.url, &cache.content, &cache.created_at],
        )?;
        Ok(())
    }

    pub fn get_web_cache(&self, url: &str) -> Result<Option<WebCache>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, content, created_at FROM web_cache WHERE url = ?",
        )?;

        let cache = stmt
            .query_row(params![url], |row| {
                Ok(WebCache {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    content: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .optional()?;

        Ok(cache)
    }

    pub fn clear_old_web_cache(&self, days: u64) -> Result<()> {
        let cutoff_time = Utc::now()
            .checked_sub_signed(chrono::Duration::days(days as i64))
            .unwrap()
            .to_rfc3339();

        self.conn.execute(
            "DELETE FROM web_cache WHERE created_at < ?",
            params![cutoff_time],
        )?;
        Ok(())
    }
}
