pub mod error;
pub mod history;

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use history::{BranchHistory, Commit, CommitId, HistoryError, RevisionSelection};

pub use error::GitError;

/// Manages revisions in an existing local Git worktree.
#[derive(Debug, Clone)]
pub struct GitManager {
    directory: PathBuf,
    branch: String,
    active_commit: Option<CommitId>,
}

impl GitManager {
    /// `None` selects the configured branch tip when `initialize` is called.
    pub fn new(
        directory: impl Into<PathBuf>,
        branch: impl Into<String>,
        active_commit: Option<CommitId>,
    ) -> Self {
        Self {
            directory: directory.into(),
            branch: branch.into(),
            active_commit,
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn active_commit(&self) -> Option<&CommitId> {
        self.active_commit.as_ref()
    }

    /// Validate the worktree, then check out the selected commit or branch tip.
    pub fn initialize(&mut self) -> Result<CommitId, GitError> {
        let expected = self.directory.canonicalize()?;
        let root = self.git(&["rev-parse", "--show-toplevel"])?;
        let actual = PathBuf::from(root.trim()).canonicalize()?;
        if actual != expected {
            return Err(GitError::RepositoryRoot { expected, actual });
        }
        let target = match &self.active_commit {
            Some(commit) => commit.clone(),
            None => self
                .revisions(&RevisionSelection::All)?
                .last()
                .ok_or_else(|| GitError::InvalidRevision("branch has no commits".into()))?
                .id
                .clone(),
        };
        self.set_revision(&target)
    }

    /// List first-parent commits on the configured branch, oldest first.
    pub fn revisions(&self, selection: &RevisionSelection) -> Result<Vec<Commit>, GitError> {
        BranchHistory {
            repository: self.directory.clone(),
            branch: self.branch.clone(),
        }
        .resolve(selection)
        .map_err(|error| match error {
            HistoryError::Git(error) => error,
        })
    }

    /// Resolve a revision to a full commit ID on this branch's first-parent history.
    pub fn validate_revision(&self, revision: &CommitId) -> Result<CommitId, GitError> {
        self.revisions(&RevisionSelection::Explicit(vec![revision.clone()]))?
            .pop()
            .map(|commit| commit.id)
            .ok_or_else(|| GitError::InvalidRevision(revision.0.clone()))
    }

    /// Check out a revision without carrying local changes into the build.
    pub fn set_revision(&mut self, revision: &CommitId) -> Result<CommitId, GitError> {
        let canonical = self.validate_revision(revision)?;
        if !self
            .git(&["status", "--porcelain=v1", "--untracked-files=normal"])?
            .is_empty()
        {
            return Err(GitError::DirtyWorktree(self.directory.clone()));
        }
        self.git(&["checkout", "--detach", &canonical.0])?;
        self.active_commit = Some(canonical.clone());
        self.git(&["submodule", "update", "--init", "--recursive", "--checkout"])?;
        Ok(canonical)
    }

    fn git(&self, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .arg("--no-pager")
            .arg("-C")
            .arg(&self.directory)
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(GitError::Git {
                repository: self.directory.clone(),
                command: args.join(" "),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
            });
        }
        String::from_utf8(output.stdout)
            .map_err(|error| GitError::InvalidGitOutput(error.to_string()))
    }
}
