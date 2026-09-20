//! # vornix-memory
//!
//! Session persistence and long-term memory for the Vornix AI coding harness.
//!
//! This crate provides two core stores backed by SQLite:
//!
//! - **[`SessionStore`]** — conversation sessions, messages, and tool-call logs.
//! - **[`MemoryStore`]** — long-term memory entries with text search.
//!
//! It also provides a **[`compaction`]** module for planning context-window
//! compaction (summarising older messages to stay within token budgets).
//!
//! ## Quick start
//!
//! ```rust,no_run
//! # async fn example() -> anyhow::Result<()> {
//! use sqlx::sqlite::SqlitePoolOptions;
//! use vornix_memory::{SessionStore, MemoryStore};
//!
//! let pool = SqlitePoolOptions::new()
//!     .connect("sqlite:vornix.db?mode=rwc")
//!     .await?;
//!
//! let sessions = SessionStore::new(pool.clone());
//! sessions.run_migrations().await?;
//!
//! let session = sessions.create_session(
//!     Some("My conversation"),
//!     Some("claude-sonnet-4-20250514"),
//!     Some("openrouter"),
//!     None,
//! ).await?;
//!
//! let memories = MemoryStore::new(pool.clone());
//! memories.run_migrations().await?;
//! # Ok(())
//! # }
//! ```

pub mod compaction;
pub mod memory;
pub mod session;

pub use compaction::{should_compact, plan_compaction, CompactionConfig, CompactionPlan, CompactionResult};
pub use memory::{MemoryEntry, MemoryStore};
pub use session::{Message, Session, SessionStats, SessionStore, ToolCallLog};