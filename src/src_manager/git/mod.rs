pub mod error;
pub mod history;

use std::{ffi::OsString, fs, path::PathBuf, process::Command};

use history::{BranchHistory, Commit, CommitId, HistoryError, RevisionSelection};

pub use error::GitError;

/// Manages a local Git worktree and its revisions.
#[derive(Debug, Clone, Getters)]
pub struct GitManager {
    #[get = "pub"]
    directory: PathBuf,
    #[get]
    branch: String,
    active_commit: Option<CommitId>,
    #[get]
    remote: Option<OsString>,
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
            remote: None,
        }
    }

    /// Clone this remote during `initialize` if the worktree is missing.
    pub fn with_remote(mut self, remote: impl Into<OsString>) -> Self {
        self.remote = Some(remote.into());
        self
    }

    pub fn active_commit(&self) -> Option<&CommitId> {
        self.active_commit.as_ref()
    }

    /// Validate the worktree, then check out the selected commit or branch tip.
    pub fn initialize(&mut self) -> Result<CommitId, GitError> {
        if let Some(remote) = &self.remote
            && !self.directory.try_exists()?
        {
            if let Some(parent) = self.directory.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent)?;
            }
            let output = Command::new("git")
                .arg("clone")
                .arg("--branch")
                .arg(&self.branch)
                .arg("--")
                .arg(remote)
                .arg(&self.directory)
                .output()?;
            if !output.status.success() {
                return Err(GitError::Git {
                    repository: self.directory.clone(),
                    command: "clone".into(),
                    status: output.status,
                    stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
                });
            }
        }
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
                .id()
                .clone(),
        };
        self.set_revision(&target)
    }

    /// List first-parent commits on the configured branch, oldest first.
    pub fn revisions(&self, selection: &RevisionSelection) -> Result<Vec<Commit>, GitError> {
        BranchHistory::new(self.directory().clone(), self.branch().clone())
            .resolve(selection)
            .map_err(|error| match error {
                HistoryError::Git(error) => error,
            })
    }

    /// Resolve a revision to a full commit ID on this branch's first-parent history.
    pub fn validate_revision(&self, revision: &CommitId) -> Result<CommitId, GitError> {
        self.revisions(&RevisionSelection::Explicit(vec![revision.clone()]))?
            .pop()
            .map(|commit| commit.id().clone())
            .ok_or_else(|| GitError::InvalidRevision(revision.id().clone()))
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
        self.git(&["checkout", "--detach", canonical.id()])?;
        self.active_commit = Some(canonical.clone());
        self.git(&["submodule", "update", "--init", "--recursive", "--checkout"])?;
        Ok(canonical)
    }

    fn git(&self, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .arg("--no-pager")
            .arg("-C")
            .arg(self.directory())
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(GitError::Git {
                repository: self.directory().clone(),
                command: args.join(" "),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
            });
        }
        String::from_utf8(output.stdout)
            .map_err(|error| GitError::InvalidGitOutput(error.to_string()))
    }
}
