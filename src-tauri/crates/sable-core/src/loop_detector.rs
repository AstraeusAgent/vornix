//! Loop detection for the agent's tool-call stream.
//!
//! Monitors a sliding window of recent tool calls and flags repetition
//! patterns before the agent wastes tokens on an infinite loop.
//!
//! Tool names and arguments are hashed using a fast non-cryptographic hash.
//! Arguments are normalised (JSON keys sorted) before hashing so that
//! equivalent calls with different key orderings are detected as duplicates.

use std::collections::{hash_map::DefaultHasher, VecDeque};
use std::hash::{Hash, Hasher};

use serde::Serialize;

// ─── Result type ─────────────────────────────────────────────────────────────

/// The outcome of a loop-detection check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LoopCheckResult {
    /// No concerning repetition detected.
    Ok,
    /// The same tool + args pattern has appeared multiple times — suspicious
    /// but not yet confirmed stuck.
    Suspicious { count: usize, tool_name: String },
    /// The pattern has hit the repeat threshold — the agent is very likely
    /// stuck in a loop.
    Stuck { count: usize, tool_name: String },
}

impl LoopCheckResult {
    /// Returns `true` if the result is `Ok`.
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    /// Returns `true` if the result is `Suspicious` or `Stuck`.
    pub fn is_concerning(&self) -> bool {
        !self.is_ok()
    }
}

// ─── Loop detector ───────────────────────────────────────────────────────────

/// Sliding-window detector for repeated tool-call patterns.
#[derive(Debug, Clone)]
pub struct LoopDetector {
    /// Circular buffer of `(tool_name_hash, args_hash)` pairs.
    recent_calls: VecDeque<(u64, u64)>,
    /// Size of the sliding window (most recent N calls).
    window_size: usize,
    /// How many identical entries within the window trigger a `Stuck` result.
    repeat_threshold: usize,
}

impl LoopDetector {
    /// Create a detector with default parameters (window=10, threshold=3).
    pub fn new() -> Self {
        Self {
            recent_calls: VecDeque::new(),
            window_size: 10,
            repeat_threshold: 3,
        }
    }

    /// Create a detector with custom parameters.
    pub fn with_params(window_size: usize, repeat_threshold: usize) -> Self {
        Self {
            recent_calls: VecDeque::new(),
            window_size,
            repeat_threshold,
        }
    }

    /// Record a tool call and check for repetition.
    ///
    /// `tool_name` is the full qualified name (e.g. `"filesystem.readFile"`).
    /// `args` is the JSON arguments object.
    pub fn check(&mut self, tool_name: &str, args: &serde_json::Value) -> LoopCheckResult {
        let name_hash = hash_string(tool_name);
        let args_hash = hash_json_value(args);
        let entry = (name_hash, args_hash);

        // Push into the sliding window.
        self.recent_calls.push_back(entry);
        if self.recent_calls.len() > self.window_size {
            self.recent_calls.pop_front();
        }

        // Count how many entries in the window match this exact pair.
        let count = self
            .recent_calls
            .iter()
            .filter(|e| **e == entry)
            .count();

        if count >= self.repeat_threshold {
            LoopCheckResult::Stuck {
                count,
                tool_name: tool_name.to_string(),
            }
        } else if count >= 2 {
            LoopCheckResult::Suspicious {
                count,
                tool_name: tool_name.to_string(),
            }
        } else {
            LoopCheckResult::Ok
        }
    }

    /// Reset the detector's history.
    pub fn reset(&mut self) {
        self.recent_calls.clear();
    }
}

impl Default for LoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Hashing helpers ─────────────────────────────────────────────────────────

/// Fast non-cryptographic hash of a string.
fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Hash a JSON value with object keys sorted so that `{"a":1,"b":2}` and
/// `{"b":2,"a":1}` produce the same hash.
fn hash_json_value(value: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_value_recursive(value, &mut hasher);
    hasher.finish()
}

fn hash_value_recursive<H: Hasher>(value: &serde_json::Value, hasher: &mut H) {
    match value {
        serde_json::Value::Null => 0u8.hash(hasher),
        serde_json::Value::Bool(b) => {
            1u8.hash(hasher);
            b.hash(hasher);
        }
        serde_json::Value::Number(n) => {
            2u8.hash(hasher);
            // Hash the string representation to avoid float precision issues.
            n.to_string().hash(hasher);
        }
        serde_json::Value::String(s) => {
            3u8.hash(hasher);
            s.hash(hasher);
        }
        serde_json::Value::Array(arr) => {
            4u8.hash(hasher);
            arr.len().hash(hasher);
            for item in arr {
                hash_value_recursive(item, hasher);
            }
        }
        serde_json::Value::Object(map) => {
            5u8.hash(hasher);
            map.len().hash(hasher);
            // Sort keys for deterministic hashing.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                key.hash(hasher);
                hash_value_recursive(&map[key], hasher);
            }
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_repetition_returns_ok() {
        let mut det = LoopDetector::with_params(5, 3);
        assert_eq!(det.check("a", &json!({"x": 1})), LoopCheckResult::Ok);
        assert_eq!(det.check("b", &json!({"x": 2})), LoopCheckResult::Ok);
        assert_eq!(det.check("c", &json!({"x": 3})), LoopCheckResult::Ok);
    }

    #[test]
    fn two_repeats_suspicious() {
        let mut det = LoopDetector::with_params(5, 3);
        det.check("read", &json!({"path": "/a"}));
        assert_eq!(
            det.check("read", &json!({"path": "/a"})),
            LoopCheckResult::Suspicious {
                count: 2,
                tool_name: "read".to_string(),
            }
        );
    }

    #[test]
    fn three_repeats_stuck() {
        let mut det = LoopDetector::with_params(5, 3);
        det.check("read", &json!({"path": "/a"}));
        det.check("read", &json!({"path": "/a"}));
        assert_eq!(
            det.check("read", &json!({"path": "/a"})),
            LoopCheckResult::Stuck {
                count: 3,
                tool_name: "read".to_string(),
            }
        );
    }

    #[test]
    fn different_args_not_counted() {
        let mut det = LoopDetector::with_params(5, 3);
        det.check("read", &json!({"path": "/a"}));
        det.check("read", &json!({"path": "/b"}));
        assert_eq!(
            det.check("read", &json!({"path": "/a"})),
            LoopCheckResult::Suspicious {
                count: 2,
                tool_name: "read".to_string(),
            }
        );
    }

    #[test]
    fn key_order_normalized() {
        let mut det = LoopDetector::with_params(5, 3);
        det.check("t", &json!({"a": 1, "b": 2}));
        det.check("t", &json!({"b": 2, "a": 1}));
        // Both should hash the same, so count = 2 → Suspicious.
        assert!(det.check("t", &json!({"a": 1, "b": 2})).is_concerning());
    }

    #[test]
    fn window_slides_out() {
        let mut det = LoopDetector::with_params(3, 3);
        det.check("a", &json!({})); // window: [a]
        det.check("b", &json!({})); // window: [a, b]
        det.check("c", &json!({})); // window: [a, b, c]
        det.check("d", &json!({})); // window: [b, c, d]  — a slid out
        // Now "a" is no longer in the window.
        assert_eq!(det.check("a", &json!({})), LoopCheckResult::Ok);
    }

    #[test]
    fn reset_clears_history() {
        let mut det = LoopDetector::with_params(5, 3);
        det.check("x", &json!({}));
        det.check("x", &json!({}));
        det.reset();
        assert_eq!(det.check("x", &json!({})), LoopCheckResult::Ok);
    }

    #[test]
    fn result_utility_methods() {
        assert!(LoopCheckResult::Ok.is_ok());
        assert!(!LoopCheckResult::Ok.is_concerning());
        assert!(LoopCheckResult::Suspicious {
            count: 2,
            tool_name: "t".into(),
        }
        .is_concerning());
        assert!(LoopCheckResult::Stuck {
            count: 3,
            tool_name: "t".into(),
        }
        .is_concerning());
    }
}
