use crate::loader::SkillSummary;

#[derive(Debug, Clone)]
pub struct SkillIndex {
    entries: Vec<SkillEntry>,
}

#[derive(Debug, Clone)]
struct SkillEntry {
    name: String,
    description: String,
    is_builtin: bool,
    tokens: Vec<String>, // Tokenized name + description for matching
}

impl SkillIndex {
    pub fn build(summaries: &[SkillSummary]) -> Self {
        let entries = summaries
            .iter()
            .map(|s| {
                let mut tokens: Vec<String> = s
                    .name
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|t| !t.is_empty())
                    .map(|t| t.to_lowercase())
                    .collect();
                tokens.extend(
                    s.description
                        .split_whitespace()
                        .map(|w| {
                            w.chars()
                                .filter(|c| c.is_alphanumeric())
                                .collect::<String>()
                                .to_lowercase()
                        })
                        .filter(|w| !w.is_empty()),
                );
                tokens.sort();
                tokens.dedup();

                SkillEntry {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    is_builtin: s.is_builtin,
                    tokens,
                }
            })
            .collect();

        Self { entries }
    }

    pub fn search(&self, query: &str) -> Vec<SkillSearchResult> {
        let query_tokens: Vec<String> = query
            .split_whitespace()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
                    .to_lowercase()
            })
            .filter(|w| !w.is_empty())
            .collect();

        if query_tokens.is_empty() {
            return self
                .entries
                .iter()
                .map(|e| SkillSearchResult {
                    name: e.name.clone(),
                    description: e.description.clone(),
                    is_builtin: e.is_builtin,
                    score: 0.0,
                })
                .collect();
        }

        let mut results: Vec<SkillSearchResult> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let mut matches = 0usize;
                for qt in &query_tokens {
                    if entry.name.to_lowercase().contains(qt.as_str())
                        || entry.description.to_lowercase().contains(qt.as_str())
                        || entry
                            .tokens
                            .iter()
                            .any(|t| t.contains(qt.as_str()))
                    {
                        matches += 1;
                    }
                }
                if matches == 0 {
                    return None;
                }
                let score = matches as f64 / query_tokens.len() as f64;
                Some(SkillSearchResult {
                    name: entry.name.clone(),
                    description: entry.description.clone(),
                    is_builtin: entry.is_builtin,
                    score,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Generates the "available skills" system prompt fragment (progressive disclosure).
    /// Only shows name + description, not the full body.
    pub fn get_system_prompt_fragment(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut out =
            String::from("## Available Skills\n\nThe following skills are available. Load a skill's full instructions with `load_skill(name)` when a task matches its description.\n\n");
        for entry in &self.entries {
            let builtin_tag = if entry.is_builtin { " [builtin]" } else { "" } ;
            out.push_str(&format!(
                "- **{}**{}: {}\n",
                entry.name, builtin_tag, entry.description
            ));
        }
        out
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct SkillSearchResult {
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_summaries() -> Vec<SkillSummary> {
        vec![
            SkillSummary {
                name: "deep-research".to_string(),
                description: "Research a topic across multiple sources".to_string(),
                path: "/test/deep-research/SKILL.md".into(),
                is_builtin: true,
            },
            SkillSummary {
                name: "pdf".to_string(),
                description: "Create, read, and manipulate PDF files".to_string(),
                path: "/test/pdf/SKILL.md".into(),
                is_builtin: true,
            },
            SkillSummary {
                name: "custom-skill".to_string(),
                description: "My custom debugging workflow".to_string(),
                path: "/test/custom-skill/SKILL.md".into(),
                is_builtin: false,
            },
        ]
    }

    #[test]
    fn test_search_exact() {
        let index = SkillIndex::build(&sample_summaries());
        let results = index.search("pdf");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "pdf");
    }

    #[test]
    fn test_search_partial() {
        let index = SkillIndex::build(&sample_summaries());
        let results = index.search("research");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "deep-research");
    }

    #[test]
    fn test_search_multiples() {
        let index = SkillIndex::build(&sample_summaries());
        let results = index.search("custom debugging");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "custom-skill");
    }

    #[test]
    fn test_system_prompt() {
        let index = SkillIndex::build(&sample_summaries());
        let prompt = index.get_system_prompt_fragment();
        assert!(prompt.contains("**deep-research**"));
        assert!(prompt.contains("[builtin]"));
        assert!(prompt.contains("load_skill"));
    }

    #[test]
    fn test_empty_index() {
        let index = SkillIndex::build(&[]);
        assert!(index.search("anything").is_empty());
        assert!(index.get_system_prompt_fragment().is_empty());
    }
}