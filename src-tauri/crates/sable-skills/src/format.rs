use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    pub path: PathBuf,
    pub is_builtin: bool,
}

impl Skill {
    pub fn parse(content: &str, path: PathBuf, is_builtin: bool) -> Result<Self> {
        let trimmed = content.trim();
        if !trimmed.starts_with("---") {
            anyhow::bail!(
                "SKILL.md at {} does not start with YAML frontmatter (---)",
                path.display()
            );
        }

        let after_first = &trimmed[3..];
        let end = after_first
            .find("---")
            .context("unterminated YAML frontmatter in SKILL.md")?;

        let yaml_str = &after_first[..end];
        let frontmatter: SkillFrontmatter =
            serde_yaml::from_str(yaml_str).context("failed to parse SKILL.md frontmatter")?;

        let body = after_first[end + 3..].trim().to_string();

        Ok(Self {
            frontmatter,
            body,
            path,
            is_builtin,
        })
    }

    pub fn name(&self) -> &str {
        &self.frontmatter.name
    }

    pub fn description(&self) -> &str {
        &self.frontmatter.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_skill() {
        let content = r#"---
name: test-skill
description: A test skill
version: "1.0"
---
# Test Skill

This is the body."#;

        let skill = Skill::parse(content, PathBuf::from("/test/SKILL.md"), true).unwrap();
        assert_eq!(skill.name(), "test-skill");
        assert_eq!(skill.description(), "A test skill");
        assert!(skill.body.contains("This is the body."));
    }

    #[test]
    fn test_parse_minimal() {
        let content = r#"---
name: minimal
description: Minimal skill
---
Body here"#;

        let skill = Skill::parse(content, PathBuf::from("/test/SKILL.md"), false).unwrap();
        assert_eq!(skill.frontmatter.version, None);
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "Just some text without frontmatter";
        assert!(Skill::parse(content, PathBuf::from("/test/SKILL.md"), true).is_err());
    }
}