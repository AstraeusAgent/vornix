//! Per-session undo history for reversible tool operations.
//!
//! Before a tool modifies a file, the runtime pushes a snapshot of the file's
//! current content into the [`UndoHistory`].  If the user (or an automated
//! rollback) wants to revert, the snapshot is used to restore the file.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors related to undo operations.
#[derive(Debug, Error)]
pub enum UndoError {
    #[error("no undo entry found for tool call {tool_call_id}")]
    EntryNotFound { tool_call_id: Uuid },

    #[error("undo history is empty")]
    Empty,
}

// ---------------------------------------------------------------------------
// UndoEntry
// ---------------------------------------------------------------------------

/// A single point-in-time snapshot of a file's content, captured before a
/// tool call modified it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoEntry {
    /// The tool call that is about to (or did) modify the file.
    pub tool_call_id: Uuid,
    /// Absolute path of the file that was snapshotted.
    pub file_path: String,
    /// Full content of the file at the time of the snapshot.
    pub pre_content: String,
    /// When the snapshot was taken.
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// UndoHistory
// ---------------------------------------------------------------------------

/// A per-session stack of undo snapshots.  Entries are ordered oldest-first
/// (index 0 is the earliest snapshot).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoHistory {
    entries: Vec<UndoEntry>,
}

impl UndoHistory {
    /// Create an empty undo history.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Push a snapshot onto the undo stack.
    ///
    /// `tool_call_id` identifies the tool call that will modify `file_path`.
    /// `pre_content` is the file's content *before* the modification.
    pub fn push_snapshot(
        &mut self,
        tool_call_id: Uuid,
        file_path: impl Into<String>,
        pre_content: impl Into<String>,
    ) {
        self.entries.push(UndoEntry {
            tool_call_id,
            file_path: file_path.into(),
            pre_content: pre_content.into(),
            timestamp: Utc::now(),
        });
    }

    /// Return all undo entries in chronological order (oldest first).
    pub fn get_undo_entries(&self) -> &[UndoEntry] {
        &self.entries
    }

    /// Return `true` if there is at least one entry that can be undone.
    pub fn can_undo(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Return the most recent undo entry without removing it.
    pub fn peek_latest(&self) -> Option<&UndoEntry> {
        self.entries.last()
    }

    /// Pop and return the most recent undo entry (LIFO order).
    pub fn pop_latest(&mut self) -> Option<UndoEntry> {
        self.entries.pop()
    }

    /// Return all undo entries for a specific file path, in chronological order.
    pub fn entries_for_file(&self, file_path: &str) -> Vec<&UndoEntry> {
        self.entries
            .iter()
            .filter(|e| e.file_path == file_path)
            .collect()
    }

    /// Return all undo entries associated with a specific tool call id.
    pub fn entries_for_tool_call(&self, tool_call_id: Uuid) -> Vec<&UndoEntry> {
        self.entries
            .iter()
            .filter(|e| e.tool_call_id == tool_call_id)
            .collect()
    }

    /// Remove and return the most recent entry for `file_path`, if any.
    pub fn pop_latest_for_file(&mut self, file_path: &str) -> Option<UndoEntry> {
        if let Some(pos) = self
            .entries
            .iter()
            .rposition(|e| e.file_path == file_path)
        {
            Some(self.entries.remove(pos))
        } else {
            None
        }
    }

    /// Clear all undo entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Return the number of undo entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if the history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_peek() {
        let mut history = UndoHistory::new();
        assert!(!history.can_undo());

        let call_id = Uuid::new_v4();
        history.push_snapshot(call_id, "/tmp/a.txt", "old content");

        assert!(history.can_undo());
        assert_eq!(history.len(), 1);

        let latest = history.peek_latest().unwrap();
        assert_eq!(latest.file_path, "/tmp/a.txt");
        assert_eq!(latest.pre_content, "old content");
        assert_eq!(latest.tool_call_id, call_id);
    }

    #[test]
    fn pop_latest_lifo() {
        let mut history = UndoHistory::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        history.push_snapshot(id1, "/tmp/a.txt", "first");
        history.push_snapshot(id2, "/tmp/b.txt", "second");

        let popped = history.pop_latest().unwrap();
        assert_eq!(popped.file_path, "/tmp/b.txt");
        assert_eq!(history.len(), 1);

        let popped = history.pop_latest().unwrap();
        assert_eq!(popped.file_path, "/tmp/a.txt");
        assert!(history.is_empty());
    }

    #[test]
    fn entries_for_file() {
        let mut history = UndoHistory::new();
        history.push_snapshot(Uuid::new_v4(), "/tmp/a.txt", "v1");
        history.push_snapshot(Uuid::new_v4(), "/tmp/b.txt", "v1");
        history.push_snapshot(Uuid::new_v4(), "/tmp/a.txt", "v2");

        let a_entries = history.entries_for_file("/tmp/a.txt");
        assert_eq!(a_entries.len(), 2);
        assert_eq!(a_entries[0].pre_content, "v1");
        assert_eq!(a_entries[1].pre_content, "v2");

        let b_entries = history.entries_for_file("/tmp/b.txt");
        assert_eq!(b_entries.len(), 1);
    }

    #[test]
    fn pop_latest_for_file() {
        let mut history = UndoHistory::new();
        history.push_snapshot(Uuid::new_v4(), "/tmp/a.txt", "v1");
        history.push_snapshot(Uuid::new_v4(), "/tmp/b.txt", "bv1");
        history.push_snapshot(Uuid::new_v4(), "/tmp/a.txt", "v2");

        let popped = history.pop_latest_for_file("/tmp/a.txt").unwrap();
        assert_eq!(popped.pre_content, "v2");
        assert_eq!(history.len(), 2);

        let popped = history.pop_latest_for_file("/tmp/a.txt").unwrap();
        assert_eq!(popped.pre_content, "v1");
        assert_eq!(history.len(), 1);

        assert!(history.pop_latest_for_file("/tmp/a.txt").is_none());
    }

    #[test]
    fn entries_for_tool_call() {
        let call_id = Uuid::new_v4();
        let mut history = UndoHistory::new();
        history.push_snapshot(call_id, "/tmp/a.txt", "a");
        history.push_snapshot(call_id, "/tmp/b.txt", "b");
        history.push_snapshot(Uuid::new_v4(), "/tmp/c.txt", "c");

        let entries = history.entries_for_tool_call(call_id);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn clear() {
        let mut history = UndoHistory::new();
        history.push_snapshot(Uuid::new_v4(), "/tmp/a.txt", "x");
        history.push_snapshot(Uuid::new_v4(), "/tmp/b.txt", "y");
        assert_eq!(history.len(), 2);

        history.clear();
        assert!(history.is_empty());
        assert!(!history.can_undo());
    }
}
