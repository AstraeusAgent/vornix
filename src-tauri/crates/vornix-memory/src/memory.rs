//! Long-term memory store.
//!
//! [`MemoryStore`] provides persistent memory entries with text search.
//! Entries are scoped to optional projects and tagged for categorization.
//!
//! The current search implementation uses SQLite `LIKE` as a baseline.
//! Embedding-based semantic search can be layered on top by populating
//! the `embedding` column and switching the `search` method to a
//! cosine-similarity scan.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

// ─── Data types ──────────────────────────────────────────────────────────────

/// A single memory entry stored for long-term recall.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub content: String,
    pub source_session_id: Option<Uuid>,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub embedding: Option<Vec<f32>>,
}

// ─── MemoryStore ─────────────────────────────────────────────────────────────

/// Persistent memory store backed by SQLite.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    pool: SqlitePool,
}

impl MemoryStore {
    /// Wrap an existing pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Access the underlying pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ── Migrations ───────────────────────────────────────────────────────

    /// Create the memory_entries table if it does not already exist.
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_entries (
                id                TEXT PRIMARY KEY NOT NULL,
                content           TEXT NOT NULL,
                source_session_id TEXT,
                project           TEXT,
                tags              TEXT NOT NULL DEFAULT '[]',
                created_at        TEXT NOT NULL,
                updated_at        TEXT NOT NULL,
                embedding         BLOB
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Creating memory_entries table")?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memory_project ON memory_entries(project)",
        )
        .execute(&self.pool)
        .await
        .context("Creating memory project index")?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memory_created ON memory_entries(created_at)",
        )
        .execute(&self.pool)
        .await
        .context("Creating memory created_at index")?;

        tracing::info!("Memory store migrations complete");
        Ok(())
    }

    // ── CRUD ─────────────────────────────────────────────────────────────

    /// Store a new memory entry and return it.
    pub async fn store(
        &self,
        content: &str,
        source_session_id: Option<Uuid>,
        project: Option<&str>,
        tags: &[String],
    ) -> Result<MemoryEntry> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let tags_json =
            serde_json::to_string(tags).context("Serializing tags to JSON")?;

        sqlx::query(
            r#"
            INSERT INTO memory_entries (id, content, source_session_id, project, tags, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(content)
        .bind(source_session_id.map(|u| u.to_string()))
        .bind(project)
        .bind(&tags_json)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .context("Inserting memory entry")?;

        Ok(MemoryEntry {
            id,
            content: content.to_string(),
            source_session_id,
            project: project.map(String::from),
            tags: tags.to_vec(),
            created_at: now,
            updated_at: now,
            embedding: None,
        })
    }

    /// Search memory entries by content text.
    ///
    /// Uses `LIKE '%query%'` as a baseline. Optionally filters by project.
    /// Returns results ordered by most recently created first, limited to
    /// `top_k` entries.
    pub async fn search(
        &self,
        query: &str,
        project: Option<&str>,
        top_k: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let pattern = format!("%{query}%");

        let rows = if let Some(proj) = project {
            sqlx::query_as::<_, MemoryRow>(
                "SELECT id, content, source_session_id, project, tags, created_at, updated_at, embedding \
                 FROM memory_entries \
                 WHERE content LIKE ?1 AND project = ?2 \
                 ORDER BY created_at DESC \
                 LIMIT ?3",
            )
            .bind(&pattern)
            .bind(proj)
            .bind(top_k as i64)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, MemoryRow>(
                "SELECT id, content, source_session_id, project, tags, created_at, updated_at, embedding \
                 FROM memory_entries \
                 WHERE content LIKE ?1 \
                 ORDER BY created_at DESC \
                 LIMIT ?2",
            )
            .bind(&pattern)
            .bind(top_k as i64)
            .fetch_all(&self.pool)
            .await
        }
        .context("Searching memory entries")?;

        rows.into_iter()
            .map(MemoryEntry::try_from)
            .collect::<Result<Vec<_>>>()
    }

    /// Get all memory entries, optionally filtered by project.
    pub async fn get_all(&self, project: Option<&str>) -> Result<Vec<MemoryEntry>> {
        let rows = if let Some(proj) = project {
            sqlx::query_as::<_, MemoryRow>(
                "SELECT id, content, source_session_id, project, tags, created_at, updated_at, embedding \
                 FROM memory_entries WHERE project = ? ORDER BY created_at DESC",
            )
            .bind(proj)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, MemoryRow>(
                "SELECT id, content, source_session_id, project, tags, created_at, updated_at, embedding \
                 FROM memory_entries ORDER BY created_at DESC",
            )
            .fetch_all(&self.pool)
            .await
        }
        .context("Fetching all memory entries")?;

        rows.into_iter()
            .map(MemoryEntry::try_from)
            .collect::<Result<Vec<_>>>()
    }

    /// Update the content and tags of a memory entry.
    pub async fn update(&self, id: Uuid, content: &str, tags: &[String]) -> Result<()> {
        let now_str = Utc::now().to_rfc3339();
        let tags_json =
            serde_json::to_string(tags).context("Serializing tags to JSON")?;

        let affected = sqlx::query(
            "UPDATE memory_entries SET content = ?, tags = ?, updated_at = ? WHERE id = ?",
        )
        .bind(content)
        .bind(&tags_json)
        .bind(&now_str)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("Updating memory entry")?
        .rows_affected();

        if affected == 0 {
            anyhow::bail!("Memory entry {id} not found");
        }
        Ok(())
    }

    /// Delete a memory entry by id.
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let affected = sqlx::query("DELETE FROM memory_entries WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("Deleting memory entry")?
            .rows_affected();

        if affected == 0 {
            anyhow::bail!("Memory entry {id} not found");
        }
        Ok(())
    }

    /// Return the total number of memory entries.
    pub async fn count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_entries")
            .fetch_one(&self.pool)
            .await
            .context("Counting memory entries")?;
        Ok(count)
    }
}

// ─── SQLx row type ───────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: String,
    content: String,
    source_session_id: Option<String>,
    project: Option<String>,
    tags: String,
    created_at: String,
    updated_at: String,
    embedding: Option<Vec<u8>>,
}

impl TryFrom<MemoryRow> for MemoryEntry {
    type Error = anyhow::Error;

    fn try_from(r: MemoryRow) -> Result<Self> {
        let tags: Vec<String> = serde_json::from_str(&r.tags).unwrap_or_default();

        let embedding = r.embedding.map(decode_f32_blob);

        Ok(Self {
            id: Uuid::parse_str(&r.id).unwrap_or_else(|_| Uuid::nil()),
            content: r.content,
            source_session_id: r
                .source_session_id
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok()),
            project: r.project,
            tags,
            created_at: DateTime::parse_from_rfc3339(&r.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&r.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            embedding,
        })
    }
}

/// Decode a BLOB of little-endian f32 values into a `Vec<f32>`.
fn decode_f32_blob(blob: Vec<u8>) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_store() -> MemoryStore {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let store = MemoryStore::new(pool);
        store.run_migrations().await.unwrap();
        store
    }

    #[tokio::test]
    async fn store_and_retrieve() {
        let store = test_store().await;
        let entry = store
            .store("The quick brown fox", None, Some("proj"), &["test".into()])
            .await
            .unwrap();

        assert_eq!(entry.content, "The quick brown fox");
        assert_eq!(entry.tags, vec!["test".to_string()]);

        let all = store.get_all(None).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, entry.id);
    }

    #[tokio::test]
    async fn search_by_content() {
        let store = test_store().await;
        store.store("Rust is fast", None, None, &[]).await.unwrap();
        store.store("Python is slow", None, None, &[]).await.unwrap();
        store.store("Rust memory safety", None, None, &[]).await.unwrap();

        let results = store.search("Rust", None, 10).await.unwrap();
        assert_eq!(results.len(), 2);

        let results = store.search("Python", None, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Python is slow");
    }

    #[tokio::test]
    async fn search_with_project_filter() {
        let store = test_store().await;
        store.store("alpha content", None, Some("proj-a"), &[]).await.unwrap();
        store.store("alpha content too", None, Some("proj-b"), &[]).await.unwrap();

        let results = store.search("alpha", Some("proj-a"), 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project.as_deref(), Some("proj-a"));
    }

    #[tokio::test]
    async fn update_entry() {
        let store = test_store().await;
        let entry = store.store("original", None, None, &["old".into()]).await.unwrap();

        store.update(entry.id, "updated", &["new".into()]).await.unwrap();

        let all = store.get_all(None).await.unwrap();
        assert_eq!(all[0].content, "updated");
        assert_eq!(all[0].tags, vec!["new".to_string()]);
    }

    #[tokio::test]
    async fn delete_entry() {
        let store = test_store().await;
        let entry = store.store("to delete", None, None, &[]).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        store.delete(entry.id).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn update_nonexistent_fails() {
        let store = test_store().await;
        let result = store.update(Uuid::new_v4(), "x", &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_nonexistent_fails() {
        let store = test_store().await;
        let result = store.delete(Uuid::new_v4()).await;
        assert!(result.is_err());
    }
}