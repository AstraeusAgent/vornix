use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use crate::format::Skill;

#[derive(Debug, Clone)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub is_builtin: bool,
}

pub struct SkillLoader {
    builtin_dir: PathBuf,
    user_dir: PathBuf,
    extra_dirs: Vec<PathBuf>,
}

impl SkillLoader {
    pub fn new(
        builtin_dir: PathBuf,
        user_dir: PathBuf,
        extra_dirs: Vec<PathBuf>,
    ) -> Self {
        Self {
            builtin_dir,
            user_dir,
            extra_dirs,
        }
    }

    pub fn default_user_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".sable")
            .join("skills")
    }

    pub fn load_all(&self) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();
        skills.extend(self.scan_dir(&self.builtin_dir, true)?);
        skills.extend(self.scan_dir(&self.user_dir, false)?);
        for dir in &self.extra_dirs {
            skills.extend(self.scan_dir(dir, false)?);
        }
        Ok(skills)
    }

    pub fn load_skill(&self, name: &str) -> Result<Option<Skill>> {
        // Search in order: user dir, builtin dir, extra dirs
        for dir in std::iter::once(&self.user_dir)
            .chain(std::iter::once(&self.builtin_dir))
            .chain(self.extra_dirs.iter())
        {
            let skill_path = dir.join(name).join("SKILL.md");
            if skill_path.exists() {
                let is_builtin = dir == &self.builtin_dir;
                let content = std::fs::read_to_string(&skill_path)
                    .with_context(|| format!("failed to read {}", skill_path.display()))?;
                return Ok(Some(Skill::parse(&content, skill_path, is_builtin)?));
            }
        }
        Ok(None)
    }

    pub fn search(&self, query: &str) -> Result<Vec<SkillSummary>> {
        let skills = self.load_all()?;
        let query_lower = query.to_lowercase();
        let summaries: Vec<SkillSummary> = skills
            .iter()
            .filter(|s| {
                s.frontmatter
                    .name
                    .to_lowercase()
                    .contains(&query_lower)
                    || s.frontmatter
                        .description
                        .to_lowercase()
                        .contains(&query_lower)
            })
            .map(|s| SkillSummary {
                name: s.frontmatter.name.clone(),
                description: s.frontmatter.description.clone(),
                path: s.path.clone(),
                is_builtin: s.is_builtin,
            })
            .collect();
        Ok(summaries)
    }

    /// List summaries for all skills (name + description only)
    pub fn list_summaries(&self) -> Result<Vec<SkillSummary>> {
        let skills = self.load_all()?;
        Ok(skills
            .iter()
            .map(|s| SkillSummary {
                name: s.frontmatter.name.clone(),
                description: s.frontmatter.description.clone(),
                path: s.path.clone(),
                is_builtin: s.is_builtin,
            })
            .collect())
    }

    pub fn create_user_skill(
        &self,
        name: &str,
        description: &str,
        body: &str,
    ) -> Result<Skill> {
        let skill_dir = self.user_dir.join(name);
        std::fs::create_dir_all(&skill_dir)
            .with_context(|| format!("failed to create skill dir {}", skill_dir.display()))?;

        let content = format!(
            "---\nname: {}\ndescription: {}\n---\n\n{}",
            name, description, body
        );
        let skill_path = skill_dir.join("SKILL.md");
        std::fs::write(&skill_path, &content)
            .with_context(|| format!("failed to write {}", skill_path.display()))?;

        Skill::parse(&content, skill_path, false)
    }

    pub fn delete_user_skill(&self, name: &str) -> Result<()> {
        let skill_dir = self.user_dir.join(name);
        if !skill_dir.exists() {
            anyhow::bail!("skill '{}' not found in user skills directory", name);
        }
        if skill_dir.join("SKILL.md").exists() {
            // Don't delete builtin skills
            if self.builtin_dir.join(name).join("SKILL.md").exists() {
                anyhow::bail!(
                    "cannot delete builtin skill '{}'",
                    name
                );
            }
        }
        std::fs::remove_dir_all(&skill_dir)
            .with_context(|| format!("failed to remove skill dir {}", skill_dir.display()))?;
        Ok(())
    }

    fn scan_dir(&self, dir: &Path, is_builtin: bool) -> Result<Vec<Skill>> {
        if !dir.exists() {
            debug!("skill directory does not exist: {}", dir.display());
            return Ok(Vec::new());
        }

        let mut skills = Vec::new();
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("failed to read dir {}", dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let skill_md = entry.path().join("SKILL.md");
            if skill_md.exists() {
                match std::fs::read_to_string(&skill_md) {
                    Ok(content) => match Skill::parse(&content, skill_md.clone(), is_builtin) {
                        Ok(skill) => skills.push(skill),
                        Err(e) => warn!(
                            "failed to parse skill at {}: {}",
                            skill_md.display(),
                            e
                        ),
                    },
                    Err(e) => warn!(
                        "failed to read {}: {}",
                        skill_md.display(),
                        e
                    ),
                }
            }
        }

        Ok(skills)
    }
}