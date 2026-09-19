use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use git2::{DiffOptions, Repository, Status, StatusOptions};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GitStatus {
    pub entries: Vec<StatusEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusEntry {
    pub path: String,
    pub status: FileStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
    Typechange,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitDiff {
    pub patches: Vec<Patch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Patch {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    pub origin: char,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitCommit {
    pub id: String,
    pub author: String,
    pub email: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub parent_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlameLine {
    pub line_no: usize,
    pub content: String,
    pub commit_id: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
}

pub struct GitEngine {
    repo: Repository,
}

impl GitEngine {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .with_context(|| format!("failed to open git repo at {}", path.display()))?;
        Ok(Self { repo })
    }

    pub fn is_repo(path: &Path) -> bool {
        Repository::discover(path).is_ok()
    }

    pub fn repo_root(&self) -> Result<&Path> {
        self.repo
            .workdir()
            .or_else(|| self.repo.path().parent())
            .context("repository has no working directory")
    }

    pub fn status(&self) -> Result<GitStatus> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true);
        let statuses = self.repo.statuses(Some(&mut opts))?;
        let mut entries = Vec::new();

        for entry in statuses.iter() {
            let path = String::from_utf8_lossy(entry.path_bytes()).to_string();
            let status = if entry.status().contains(Status::CONFLICTED) {
                FileStatus::Conflicted
            } else if entry.status().contains(Status::WT_NEW) {
                FileStatus::Untracked
            } else if entry.status().contains(Status::INDEX_NEW) {
                FileStatus::Added
            } else if entry.status().contains(Status::WT_MODIFIED)
                || entry.status().contains(Status::INDEX_MODIFIED)
            {
                FileStatus::Modified
            } else if entry.status().contains(Status::WT_DELETED)
                || entry.status().contains(Status::INDEX_DELETED)
            {
                FileStatus::Deleted
            } else if entry.status().contains(Status::WT_RENAMED)
                || entry.status().contains(Status::INDEX_RENAMED)
            {
                FileStatus::Renamed
            } else if entry.status().contains(Status::WT_TYPECHANGE)
                || entry.status().contains(Status::INDEX_TYPECHANGE)
            {
                FileStatus::Typechange
            } else {
                continue;
            };
            entries.push(StatusEntry { path, status });
        }

        Ok(GitStatus { entries })
    }

    pub fn diff(&self, staged: bool) -> Result<GitDiff> {
        let diff = if staged {
            let head = self.repo.head()?.peel_to_tree()?;
            let mut index = self.repo.index()?;
            index.write()?;
            let index_tree = index.write_tree()?;
            let index_tree_obj = self.repo.find_tree(index_tree)?;
            self.repo
                .diff_tree_to_tree(Some(&head), Some(&index_tree_obj), None)?
        } else {
            let mut opts = DiffOptions::new();
            opts.include_untracked(true);
            self.repo
                .diff_index_to_workdir(None, Some(&mut opts))?
        };

        self.parse_diff(diff)
    }

    pub fn diff_against(&self, ref_name: &str) -> Result<GitDiff> {
        let obj = self
            .repo
            .revparse_single(ref_name)
            .with_context(|| format!("failed to resolve ref '{}'", ref_name))?;
        let tree = obj.peel_to_tree()?;
        let mut opts = DiffOptions::new();
        opts.include_untracked(true);
        let diff = self
            .repo
            .diff_tree_to_workdir(Some(&tree), Some(&mut opts))?;
        self.parse_diff(diff)
    }

    fn parse_diff(&self, diff: git2::Diff<'_>) -> Result<GitDiff> {
        let diff_stats = diff.deltas();
        let num_deltas = diff_stats.len();

        let mut patches = Vec::new();

        // Collect delta metadata first
        let delta_meta: Vec<_> = (0..num_deltas)
            .filter_map(|i| {
                let delta = diff.get_delta(i)?;
                let old_path = delta.old_file().path().map(|p| p.to_string_lossy().to_string());
                let new_path = delta.new_file().path().map(|p| p.to_string_lossy().to_string());
                Some((old_path, new_path))
            })
            .collect();

        // Use diff.print to iterate over each delta's lines
        let mut current_patch_idx: usize = 0;
        let mut current_patches: Vec<Option<Patch>> = delta_meta
            .iter()
            .map(|(old, new)| {
                Some(Patch {
                    old_path: old.clone(),
                    new_path: new.clone(),
                    hunks: Vec::new(),
                })
            })
            .collect();

        diff.print(git2::DiffFormat::Patch, |delta, hunk, line| {
            let idx = delta.new_file().path().and_then(|p| {
                delta_meta.iter().position(|(_, n)| {
                    n.as_ref().map(|s| s.as_str()) == Some(&p.to_string_lossy())
                })
            });

            if let Some(patch) = idx.and_then(|i| current_patches[i].as_mut()) {
                if let Some(h) = hunk {
                    let header = String::from_utf8_lossy(h.header()).to_string();
                    let last = patch.hunks.last_mut();
                    if last.is_none() || last.as_ref().map(|lh| lh.header != header).unwrap_or(true) {
                        patch.hunks.push(Hunk {
                            header,
                            lines: Vec::new(),
                        });
                    }
                }
                if let Some(last_hunk) = patch.hunks.last_mut() {
                    let origin = line.origin();
                    let content = String::from_utf8_lossy(line.content()).to_string();
                    last_hunk.lines.push(DiffLine { origin, content });
                }
            }
            true
        })
        .ok();

        patches = current_patches.into_iter().flatten().collect();

        Ok(GitDiff { patches })
    }

    pub fn log(&self, limit: usize) -> Result<Vec<GitCommit>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut commits = Vec::new();
        for oid_result in revwalk.take(limit) {
            let oid = oid_result?;
            let commit = self.repo.find_commit(oid)?;
            let author = commit.author();

            let parent_ids: Vec<String> =
                (0..commit.parent_count())
                    .filter_map(|i| commit.parent_id(i).ok())
                    .map(|id| id.to_string())
                    .collect();

            let timestamp = DateTime::from_timestamp(commit.time().seconds(), 0)
                .unwrap_or_default()
                .with_timezone(&Utc);

            commits.push(GitCommit {
                id: oid.to_string(),
                author: author.name().unwrap_or("unknown").to_string(),
                email: author.email().unwrap_or("").to_string(),
                message: commit
                    .message()
                    .unwrap_or("")
                    .trim_end()
                    .to_string(),
                timestamp,
                parent_ids,
            });
        }

        Ok(commits)
    }

    pub fn blame(&self, path: &Path) -> Result<Vec<BlameLine>> {
        let blame = self.repo.blame_file(path, None)?;

        // Get file content from working tree
        let content = std::fs::read_to_string(
            self.repo
                .workdir()
                .context("no working directory")?
                .join(path),
        )
        .unwrap_or_default();

        let mut lines = Vec::new();
        for (line_no, line_text) in content.lines().enumerate() {
            if let Some(hunk) = blame.get_line(line_no + 1) {
                let commit_id = hunk.final_commit_id().to_string();
                let sig = hunk.final_signature();
                let timestamp =
                    DateTime::from_timestamp(sig.when().seconds(), 0)
                        .unwrap_or_default()
                        .with_timezone(&Utc);
                lines.push(BlameLine {
                    line_no: line_no + 1,
                    content: line_text.to_string(),
                    commit_id,
                    author: sig.name().unwrap_or("unknown").to_string(),
                    timestamp,
                });
            }
        }

        Ok(lines)
    }

    pub fn stage(&self, paths: &[&Path]) -> Result<()> {
        let mut index = self.repo.index()?;
        for path in paths {
            index.add_path(path)?;
        }
        index.write()?;
        Ok(())
    }

    pub fn unstage(&self, paths: &[&Path]) -> Result<()> {
        let mut index = self.repo.index()?;
        for path in paths {
            let _ = index.remove(path, 0);
        }
        index.write()?;
        Ok(())
    }

    pub fn commit(&self, message: &str) -> Result<GitCommit> {
        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let signature = self.repo.signature()?;
        let parent_commit = match self.repo.head() {
            Ok(head) => Some(head.peel_to_commit()?),
            Err(_) => None,
        };
        let parents: Vec<&git2::Commit<'_>> = match &parent_commit {
            Some(c) => vec![c],
            None => vec![],
        };

        let oid = self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )?;

        let commit = self.repo.find_commit(oid)?;
        let author = commit.author();
        let timestamp = DateTime::from_timestamp(commit.time().seconds(), 0)
            .unwrap_or_default()
            .with_timezone(&Utc);

        Ok(GitCommit {
            id: oid.to_string(),
            author: author.name().unwrap_or("unknown").to_string(),
            email: author.email().unwrap_or("").to_string(),
            message: message.to_string(),
            timestamp,
            parent_ids: (0..commit.parent_count())
                .filter_map(|i| commit.parent_id(i).ok())
                .map(|id| id.to_string())
                .collect(),
        })
    }

    pub fn branch_list(&self) -> Result<Vec<BranchInfo>> {
        let branches = self.repo.branches(None)?;
        let head = self.repo.head().ok();
        let head_name = head.as_ref().and_then(|h| h.shorthand());

        let mut list = Vec::new();
        for branch_result in branches {
            let (branch, branch_type) = branch_result?;
            let name = branch
                .name()?
                .unwrap_or("<invalid utf-8>")
                .to_string();
            let is_current = branch_type == git2::BranchType::Local
                && head_name.map_or(false, |h| h == name);
            let is_remote = branch_type == git2::BranchType::Remote;

            let upstream = branch
                .upstream()
                .ok()
                .and_then(|u| u.name().ok().flatten().map(|s| s.to_string()));

            list.push(BranchInfo {
                name,
                is_current,
                is_remote,
                upstream,
            });
        }

        Ok(list)
    }

    pub fn branch_create(&self, name: &str) -> Result<()> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        self.repo.branch(name, &commit, false)?;
        Ok(())
    }

    pub fn branch_switch(&self, name: &str) -> Result<()> {
        let obj = self
            .repo
            .revparse_single(&format!("refs/heads/{}", name))?;
        self.repo.checkout_tree(&obj, None)?;
        self.repo
            .set_head(&format!("refs/heads/{}", name))?;
        Ok(())
    }

    pub fn stash(&mut self) -> Result<()> {
        let signature = self.repo.signature()?;
        self.repo
            .stash_save(&signature, "sable auto-stash", None)?;
        Ok(())
    }

    pub fn stash_pop(&mut self) -> Result<()> {
        self.repo.stash_pop(0, None)?;
        Ok(())
    }

    pub fn current_branch(&self) -> Result<Option<String>> {
        let head = self.repo.head()?;
        Ok(head.shorthand().map(|s| s.to_string()))
    }

    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        let status = self.status()?;
        Ok(!status.entries.is_empty())
    }
}