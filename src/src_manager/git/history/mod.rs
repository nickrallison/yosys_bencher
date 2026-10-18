use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use super::GitError;

mod error;
pub use error::HistoryError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchHistory {
    pub repository: PathBuf,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RevisionSelection {
    All,
    Explicit(Vec<CommitId>),
    Since(CommitId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub id: CommitId,
    pub committed_at: SystemTime,
    pub author: String,
    pub commit_message: String,
}

impl BranchHistory {
    pub fn resolve(&self, selection: &RevisionSelection) -> Result<Vec<Commit>, HistoryError> {
        let branch =
            if self.branch.starts_with("refs/heads/") || self.branch.starts_with("refs/remotes/") {
                self.branch.clone()
            } else {
                format!("refs/heads/{}", self.branch)
            };
        let tip = self.resolve_commit(&branch)?;
        let output = self.git(&[
            "log",
            "--no-patch",
            "--no-color",
            "--no-decorate",
            "--no-notes",
            "--no-show-signature",
            "--first-parent",
            "--reverse",
            "-z",
            "--encoding=UTF-8",
            "--format=%H%x00%ct%x00%an <%ae>%x00%B",
            &tip.0,
            "--",
        ])?;
        let commits = parse_log(&output)?;
        let positions: HashMap<_, _> = commits
            .iter()
            .enumerate()
            .map(|(index, commit)| (commit.id.0.as_str(), index))
            .collect();
        let position = |id: &CommitId| -> Result<usize, GitError> {
            positions.get(id.0.as_str()).copied().ok_or_else(|| {
                GitError::InvalidRevision(format!(
                    "{} is not on the first-parent history of {}",
                    id.0, self.branch,
                ))
            })
        };
        match selection {
            RevisionSelection::All => Ok(commits),
            RevisionSelection::Since(anchor) => {
                let anchor = self.resolve_commit(&anchor.0)?;
                Ok(commits[position(&anchor)? + 1..].to_vec())
            }
            RevisionSelection::Explicit(requested) => {
                let mut seen = HashSet::new();
                let mut selected = Vec::with_capacity(requested.len());
                for requested in requested {
                    let id = self.resolve_commit(&requested.0)?;
                    let index = position(&id)?;
                    if !seen.insert(id.0.clone()) {
                        return Err(GitError::InvalidRevision(format!(
                            "duplicate commit {}",
                            id.0
                        ))
                        .into());
                    }
                    selected.push(commits[index].clone());
                }
                Ok(selected)
            }
        }
    }

    fn resolve_commit(&self, revision: &str) -> Result<CommitId, HistoryError> {
        let revision = format!("{revision}^{{commit}}");
        let output = self.git(&["rev-parse", "--verify", "--end-of-options", &revision])?;
        parse_id(text(&output)?.trim())
    }

    fn git(&self, args: &[&str]) -> Result<Vec<u8>, GitError> {
        let output = Command::new("git")
            .arg("--no-pager")
            .arg("-C")
            .arg(&self.repository)
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(GitError::Git {
                repository: self.repository.clone(),
                command: args.join(" "),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
            });
        }
        Ok(output.stdout)
    }
}

fn text(bytes: &[u8]) -> Result<&str, HistoryError> {
    std::str::from_utf8(bytes).map_err(|error| GitError::InvalidGitOutput(error.to_string()).into())
}

fn parse_id(id: &str) -> Result<CommitId, HistoryError> {
    if !matches!(id.len(), 40 | 64) || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(GitError::InvalidGitOutput(format!("invalid object ID {id:?}")).into());
    }
    Ok(CommitId(id.into()))
}

fn parse_log(output: &[u8]) -> Result<Vec<Commit>, HistoryError> {
    let fields: Vec<_> = text(output)?
        .strip_suffix('\0')
        .ok_or_else(|| GitError::InvalidGitOutput("unterminated commit record".into()))?
        .split('\0')
        .collect();
    if !fields.len().is_multiple_of(4) {
        return Err(GitError::InvalidGitOutput("incomplete commit record".into()).into());
    }
    fields
        .chunks_exact(4)
        .map(|fields| {
            let seconds: i64 = fields[1].parse().map_err(|_| {
                GitError::InvalidGitOutput(format!("invalid timestamp {:?}", fields[1]))
            })?;
            let duration = Duration::from_secs(seconds.unsigned_abs());
            let committed_at = if seconds < 0 {
                UNIX_EPOCH.checked_sub(duration)
            } else {
                UNIX_EPOCH.checked_add(duration)
            }
            .ok_or_else(|| GitError::InvalidGitOutput("timestamp out of range".into()))?;
            Ok(Commit {
                id: parse_id(fields[0])?,
                committed_at,
                author: fields[2].into(),
                commit_message: fields[3].into(),
            })
        })
        .collect()
}
