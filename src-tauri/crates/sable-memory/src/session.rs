//! Session persistence — conversations, messages, and tool-call logs.
//!
//! [`SessionStore`] wraps a [`SqlitePool`] and manages three tables:
//!
//! | Table           | Purpose                            |
//! |-----------------|------------------------------------|
//! | `sessions`      | One row per conversation           |
//! | `messages`      | Chat messages within a session     |
//! | `tool_call_logs`| Detailed tool invocation records   |
//!
//! Call [`SessionStore::run_migrations`] once at startup to ensure the
//! schema exists (all statements are `CREATE TABLE IF NOT EXISTS`).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

// ─── Data types ──────────────────────────────────────────────────────────────

/// A conversation session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub project_path: Option<String>,
    pub metadata: serde_json::Value,
}

/// A single chat message within a session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub token_count: Option<i64>,
    pub model_id: Option<String>,
}

/// A record of a tool invocation attached to a message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCallLog {
    pub id: Uuid,
    pub session_id: Uuid,
    pub message_id: Uuid,
    pub tool_name: String,
    pub arguments_json: String,
    pub result_json: Option<String>,
    pub success: Option<bool>,
    pub duration_ms: Option<i64>,
    pub tier: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Aggregate statistics for a session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionStats {
    pub total_messages: i64,
    pub total_tool_calls: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub estimated_cost: f64,
}

// ─── SessionStore ────────────────────────────────────────────────────────────

/// Persistent session store backed by SQLite.
#[derive(Debug, Clone)]
pub struct SessionStore {
    pool: SqlitePool,
}

impl SessionStore {
    /// Wrap an existing pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Access the underlying pool (e.g. for passing to [`MemoryStore`]).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ── Migrations ───────────────────────────────────────────────────────

    /// Create all required tables if they do not already exist.
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY NOT NULL,
                title       TEXT,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                model_id    TEXT,
                provider_id TEXT,
                project_path TEXT,
                metadata    TEXT NOT NULL DEFAULT '{}'
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Creating sessions table")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id                TEXT PRIMARY KEY NOT NULL,
                session_id        TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                role              TEXT NOT NULL,
                content           TEXT NOT NULL DEFAULT '',
                reasoning_content TEXT,
                tool_calls_json   TEXT,
                tool_call_id      TEXT,
                timestamp         TEXT NOT NULL,
                token_count       INTEGER,
                model_id          TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Creating messages table")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tool_call_logs (
                id             TEXT PRIMARY KEY NOT NULL,
                session_id     TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                message_id     TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
                tool_name      TEXT NOT NULL,
                arguments_json TEXT NOT NULL,
                result_json    TEXT,
                success        INTEGER,
                duration_ms    INTEGER,
                tier           TEXT,
                timestamp      TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Creating tool_call_logs table")?;

        // Indexes for common query patterns.
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id)")
            .execute(&self.pool)
            .await
            .context("Creating messages session_id index")?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp)")
            .execute(&self.pool)
            .await
            .context("Creating messages timestamp index")?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_call_logs_session_id ON tool_call_logs(session_id)")
            .execute(&self.pool)
            .await
            .context("Creating tool_call_logs session_id index")?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_call_logs_message_id ON tool_call_logs(message_id)")
            .execute(&self.pool)
            .await
            .context("Creating tool_call_logs message_id index")?;

        tracing::info!("Session store migrations complete");
        Ok(())
    }

    // ── Session CRUD ─────────────────────────────────────────────────────

    /// Create a new session and return it.
    pub async fn create_session(
        &self,
        title: Option<&str>,
        model_id: Option<&str>,
        provider_id: Option<&str>,
        project_path: Option<&str>,
    ) -> Result<Session> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO sessions (id, title, created_at, updated_at, model_id, provider_id, project_path, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, '{}')
            "#,
        )
        .bind(id.to_string())
        .bind(title)
        .bind(&now_str)
        .bind(&now_str)
        .bind(model_id)
        .bind(provider_id)
        .bind(project_path)
        .execute(&self.pool)
        .await
        .context("Inserting new session")?;

        Ok(Session {
            id,
            title: title.map(String::from),
            created_at: now,
            updated_at: now,
            model_id: model_id.map(String::from),
            provider_id: provider_id.map(String::from),
            project_path: project_path.map(String::from),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        })
    }

    /// Retrieve a single session by id.
    pub async fn get_session(&self, id: Uuid) -> Result<Option<Session>> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, title, created_at, updated_at, model_id, provider_id, project_path, metadata FROM sessions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .context("Fetching session")?;

        Ok(row.map(Session::from))
    }

    /// List sessions ordered by most recently updated first.
    pub async fn list_sessions(&self, limit: i64, offset: i64) -> Result<Vec<Session>> {
        let rows = sqlx::query_as::<_, SessionRow>(
            "SELECT id, title, created_at, updated_at, model_id, provider_id, project_path, metadata \
             FROM sessions ORDER BY updated_at DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .context("Listing sessions")?;

        Ok(rows.into_iter().map(Session::from).collect())
    }

    /// Search sessions by title or by content of their messages.
    ///
    /// Uses `LIKE` with `%query%` matching.
    pub async fn search_sessions(&self, query: &str) -> Result<Vec<Session>> {
        let pattern = format!("%{query}%");

        let rows = sqlx::query_as::<_, SessionRow>(
            "SELECT DISTINCT s.id, s.title, s.created_at, s.updated_at, s.model_id, \
             s.provider_id, s.project_path, s.metadata \
             FROM sessions s \
             LEFT JOIN messages m ON m.session_id = s.id \
             WHERE s.title LIKE ?1 OR m.content LIKE ?2 \
             ORDER BY s.updated_at DESC",
        )
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .context("Searching sessions")?;

        Ok(rows.into_iter().map(Session::from).collect())
    }

    /// Update the title of a session.
    pub async fn update_session_title(&self, id: Uuid, title: &str) -> Result<()> {
        let now_str = Utc::now().to_rfc3339();
        let affected = sqlx::query(
            "UPDATE sessions SET title = ?, updated_at = ? WHERE id = ?",
        )
        .bind(title)
        .bind(&now_str)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("Updating session title")?
        .rows_affected();

        if affected == 0 {
            anyhow::bail!("Session {id} not found");
        }
        Ok(())
    }

    /// Delete a session and all its messages (cascading).
    pub async fn delete_session(&self, id: Uuid) -> Result<()> {
        // Delete messages first for explicit clarity (ON DELETE CASCADE is
        // also defined, but explicit is better).
        sqlx::query("DELETE FROM tool_call_logs WHERE session_id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("Deleting tool call logs for session")?;

        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("Deleting messages for session")?;

        let affected = sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("Deleting session")?
            .rows_affected();

        if affected == 0 {
            anyhow::bail!("Session {id} not found");
        }
        Ok(())
    }

    // ── Messages ─────────────────────────────────────────────────────────

    /// Append a new message to a session.
    pub async fn append_message(
        &self,
        session_id: Uuid,
        role: &str,
        content: &str,
        reasoning_content: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
        token_count: Option<i64>,
        model_id: Option<&str>,
    ) -> Result<Message> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO messages (id, session_id, role, content, reasoning_content, tool_calls_json, tool_call_id, timestamp, token_count, model_id)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(session_id.to_string())
        .bind(role)
        .bind(content)
        .bind(reasoning_content)
        .bind(tool_calls_json)
        .bind(tool_call_id)
        .bind(&now_str)
        .bind(token_count)
        .bind(model_id)
        .execute(&self.pool)
        .await
        .context("Inserting message")?;

        // Bump session.updated_at.
        sqlx::query("UPDATE sessions SET updated_at = ? WHERE id = ?")
            .bind(&now_str)
            .bind(session_id.to_string())
            .execute(&self.pool)
            .await
            .context("Bumping session updated_at")?;

        Ok(Message {
            id,
            session_id,
            role: role.to_string(),
            content: content.to_string(),
            reasoning_content: reasoning_content.map(String::from),
            tool_calls_json: tool_calls_json.map(String::from),
            tool_call_id: tool_call_id.map(String::from),
            timestamp: now,
            token_count,
            model_id: model_id.map(String::from),
        })
    }

    /// Get messages for a session, ordered by timestamp (oldest first),
    /// with pagination.
    pub async fn get_messages(
        &self,
        session_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, session_id, role, content, reasoning_content, tool_calls_json, \
             tool_call_id, timestamp, token_count, model_id \
             FROM messages WHERE session_id = ? ORDER BY timestamp ASC LIMIT ? OFFSET ?",
        )
        .bind(session_id.to_string())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .context("Fetching messages")?;

        Ok(rows.into_iter().map(Message::from).collect())
    }

    /// Get all messages for a session (no pagination).
    pub async fn get_all_messages(&self, session_id: Uuid) -> Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, session_id, role, content, reasoning_content, tool_calls_json, \
             tool_call_id, timestamp, token_count, model_id \
             FROM messages WHERE session_id = ? ORDER BY timestamp ASC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await
        .context("Fetching all messages")?;

        Ok(rows.into_iter().map(Message::from).collect())
    }

    // ── Tool call logs ───────────────────────────────────────────────────

    /// Log a tool call attached to a specific message.
    pub async fn log_tool_call(
        &self,
        session_id: Uuid,
        message_id: Uuid,
        tool_name: &str,
        arguments_json: &str,
        result_json: Option<&str>,
        success: Option<bool>,
        duration_ms: Option<i64>,
        tier: Option<&str>,
    ) -> Result<ToolCallLog> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        // Store success as INTEGER: 1 = true, 0 = false, NULL = unknown.
        let success_int: Option<i64> = success.map(|b| if b { 1 } else { 0 });

        sqlx::query(
            r#"
            INSERT INTO tool_call_logs (id, session_id, message_id, tool_name, arguments_json, result_json, success, duration_ms, tier, timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(session_id.to_string())
        .bind(message_id.to_string())
        .bind(tool_name)
        .bind(arguments_json)
        .bind(result_json)
        .bind(success_int)
        .bind(duration_ms)
        .bind(tier)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .context("Inserting tool call log")?;

        Ok(ToolCallLog {
            id,
            session_id,
            message_id,
            tool_name: tool_name.to_string(),
            arguments_json: arguments_json.to_string(),
            result_json: result_json.map(String::from),
            success,
            duration_ms,
            tier: tier.map(String::from),
            timestamp: now,
        })
    }

    /// Retrieve tool call logs for a session, optionally filtered by message.
    pub async fn get_tool_calls(
        &self,
        session_id: Uuid,
        message_id: Option<Uuid>,
    ) -> Result<Vec<ToolCallLog>> {
        let rows = if let Some(mid) = message_id {
            sqlx::query_as::<_, ToolCallLogRow>(
                "SELECT id, session_id, message_id, tool_name, arguments_json, \
                 result_json, success, duration_ms, tier, timestamp \
                 FROM tool_call_logs WHERE session_id = ? AND message_id = ? \
                 ORDER BY timestamp ASC",
            )
            .bind(session_id.to_string())
            .bind(mid.to_string())
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, ToolCallLogRow>(
                "SELECT id, session_id, message_id, tool_name, arguments_json, \
                 result_json, success, duration_ms, tier, timestamp \
                 FROM tool_call_logs WHERE session_id = ? \
                 ORDER BY timestamp ASC",
            )
            .bind(session_id.to_string())
            .fetch_all(&self.pool)
            .await
        }
        .context("Fetching tool call logs")?;

        Ok(rows.into_iter().map(ToolCallLog::from).collect())
    }

    // ── Stats ────────────────────────────────────────────────────────────

    /// Compute aggregate statistics for a session.
    ///
    /// - **total_messages**: count of messages in the session.
    /// - **total_tool_calls**: count of tool call logs.
    /// - **total_input_tokens**: sum of `token_count` for `user` and `system` roles.
    /// - **total_output_tokens**: sum of `token_count` for `assistant` and `tool` roles.
    /// - **estimated_cost**: rough estimate at $0.003 / 1K input tokens and
    ///   $0.015 / 1K output tokens (adjust as needed).
    pub async fn get_session_stats(&self, session_id: Uuid) -> Result<SessionStats> {
        // Total messages.
        let total_messages: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?",
        )
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await
        .context("Counting messages")?;

        // Total tool calls.
        let total_tool_calls: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tool_call_logs WHERE session_id = ?",
        )
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await
        .context("Counting tool calls")?;

        // Input tokens (user + system messages).
        let total_input_tokens: Option<i64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(token_count), 0) FROM messages \
             WHERE session_id = ? AND role IN ('user', 'system')",
        )
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await
        .context("Summing input tokens")?;

        // Output tokens (assistant + tool messages).
        let total_output_tokens: Option<i64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(token_count), 0) FROM messages \
             WHERE session_id = ? AND role IN ('assistant', 'tool')",
        )
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await
        .context("Summing output tokens")?;

        let input = total_input_tokens.unwrap_or(0);
        let output = total_output_tokens.unwrap_or(0);

        // Rough cost estimate: $0.003/1K input, $0.015/1K output.
        let estimated_cost =
            (input as f64 / 1000.0) * 0.003 + (output as f64 / 1000.0) * 0.015;

        Ok(SessionStats {
            total_messages,
            total_tool_calls,
            total_input_tokens: input,
            total_output_tokens: output,
            estimated_cost,
        })
    }
}

// ─── SQLx row types ──────────────────────────────────────────────────────────
// These intermediate structs match the SQLite column layout exactly and are
// converted into the public domain types.

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    title: Option<String>,
    created_at: String,
    updated_at: String,
    model_id: Option<String>,
    provider_id: Option<String>,
    project_path: Option<String>,
    metadata: String,
}

impl From<SessionRow> for Session {
    fn from(r: SessionRow) -> Self {
        Self {
            id: Uuid::parse_str(&r.id).unwrap_or_else(|_| Uuid::nil()),
            title: r.title,
            created_at: DateTime::parse_from_rfc3339(&r.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&r.updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            model_id: r.model_id,
            provider_id: r.provider_id,
            project_path: r.project_path,
            metadata: serde_json::from_str(&r.metadata)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        }
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    session_id: String,
    role: String,
    content: String,
    reasoning_content: Option<String>,
    tool_calls_json: Option<String>,
    tool_call_id: Option<String>,
    timestamp: String,
    token_count: Option<i64>,
    model_id: Option<String>,
}

impl From<MessageRow> for Message {
    fn from(r: MessageRow) -> Self {
        Self {
            id: Uuid::parse_str(&r.id).unwrap_or_else(|_| Uuid::nil()),
            session_id: Uuid::parse_str(&r.session_id).unwrap_or_else(|_| Uuid::nil()),
            role: r.role,
            content: r.content,
            reasoning_content: r.reasoning_content,
            tool_calls_json: r.tool_calls_json,
            tool_call_id: r.tool_call_id,
            timestamp: DateTime::parse_from_rfc3339(&r.timestamp)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            token_count: r.token_count,
            model_id: r.model_id,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ToolCallLogRow {
    id: String,
    session_id: String,
    message_id: String,
    tool_name: String,
    arguments_json: String,
    result_json: Option<String>,
    success: Option<i64>,
    duration_ms: Option<i64>,
    tier: Option<String>,
    timestamp: String,
}

impl From<ToolCallLogRow> for ToolCallLog {
    fn from(r: ToolCallLogRow) -> Self {
        Self {
            id: Uuid::parse_str(&r.id).unwrap_or_else(|_| Uuid::nil()),
            session_id: Uuid::parse_str(&r.session_id).unwrap_or_else(|_| Uuid::nil()),
            message_id: Uuid::parse_str(&r.message_id).unwrap_or_else(|_| Uuid::nil()),
            tool_name: r.tool_name,
            arguments_json: r.arguments_json,
            result_json: r.result_json,
            success: r.success.map(|v| v != 0),
            duration_ms: r.duration_ms,
            tier: r.tier,
            timestamp: DateTime::parse_from_rfc3339(&r.timestamp)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_store() -> SessionStore {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let store = SessionStore::new(pool);
        store.run_migrations().await.unwrap();
        store
    }

    #[tokio::test]
    async fn create_and_get_session() {
        let store = test_store().await;
        let session = store
            .create_session(Some("Test"), Some("model-a"), Some("prov"), Some("/project"))
            .await
            .unwrap();

        assert_eq!(session.title.as_deref(), Some("Test"));

        let fetched = store.get_session(session.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.model_id.as_deref(), Some("model-a"));
    }

    #[tokio::test]
    async fn list_and_search_sessions() {
        let store = test_store().await;
        store.create_session(Some("Alpha"), None, None, None).await.unwrap();
        store.create_session(Some("Beta search"), None, None, None).await.unwrap();
        store.create_session(None, None, None, None).await.unwrap();

        let all = store.list_sessions(100, 0).await.unwrap();
        assert_eq!(all.len(), 3);

        let found = store.search_sessions("search").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title.as_deref(), Some("Beta search"));
    }

    #[tokio::test]
    async fn update_and_delete_session() {
        let store = test_store().await;
        let session = store.create_session(None, None, None, None).await.unwrap();

        store.update_session_title(session.id, "New Title").await.unwrap();
        let s = store.get_session(session.id).await.unwrap().unwrap();
        assert_eq!(s.title.as_deref(), Some("New Title"));

        store.delete_session(session.id).await.unwrap();
        assert!(store.get_session(session.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn append_and_get_messages() {
        let store = test_store().await;
        let session = store.create_session(None, None, None, None).await.unwrap();

        let m1 = store
            .append_message(session.id, "user", "Hello", None, None, None, Some(10), None)
            .await
            .unwrap();
        let m2 = store
            .append_message(session.id, "assistant", "Hi there", None, None, None, Some(15), Some("m"))
            .await
            .unwrap();

        let msgs = store.get_all_messages(session.id).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].id, m1.id);
        assert_eq!(msgs[1].id, m2.id);

        let paged = store.get_messages(session.id, 1, 1).await.unwrap();
        assert_eq!(paged.len(), 1);
        assert_eq!(paged[0].id, m2.id);
    }

    #[tokio::test]
    async fn tool_call_logging() {
        let store = test_store().await;
        let session = store.create_session(None, None, None, None).await.unwrap();
        let msg = store
            .append_message(session.id, "assistant", "", None, Some("[{\"id\":\"c1\"}]"), None, None, None)
            .await
            .unwrap();

        let log = store
            .log_tool_call(
                session.id,
                msg.id,
                "read_file",
                r#"{"path":"/tmp/test"}"#,
                Some(r#"{"content":"hello"}"#),
                Some(true),
                Some(42),
                Some("safe"),
            )
            .await
            .unwrap();

        assert_eq!(log.tool_name, "read_file");
        assert_eq!(log.success, Some(true));

        let logs = store.get_tool_calls(session.id, Some(msg.id)).await.unwrap();
        assert_eq!(logs.len(), 1);

        let all_logs = store.get_tool_calls(session.id, None).await.unwrap();
        assert_eq!(all_logs.len(), 1);
    }

    #[tokio::test]
    async fn session_stats() {
        let store = test_store().await;
        let session = store.create_session(None, None, None, None).await.unwrap();

        store.append_message(session.id, "user", "q1", None, None, None, Some(100), None).await.unwrap();
        store.append_message(session.id, "assistant", "a1", None, None, None, Some(200), None).await.unwrap();
        store.append_message(session.id, "user", "q2", None, None, None, Some(50), None).await.unwrap();
        store.append_message(session.id, "assistant", "a2", None, None, None, Some(300), None).await.unwrap();

        let stats = store.get_session_stats(session.id).await.unwrap();
        assert_eq!(stats.total_messages, 4);
        assert_eq!(stats.total_input_tokens, 150);  // 100 + 50
        assert_eq!(stats.total_output_tokens, 500);  // 200 + 300
        assert!(stats.estimated_cost > 0.0);
    }
}