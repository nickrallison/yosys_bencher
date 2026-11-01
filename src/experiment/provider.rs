use std::{fs, io, path::PathBuf};

use super::ExperimentError;
use crate::{
    src_manager::{
        build::CcacheCMakeBuilder,
        git::{
            GitManager,
            history::{Commit, RevisionSelection},
        },
    },
    tools::yosys::Yosys,
};

pub trait ToolProvider {
    type Tool;

    fn set_progress(&mut self, _progress: crate::progress::Progress) {}

    fn repository(&self) -> String {
        "local".into()
    }
    fn history(&self) -> String {
        "first-parent".into()
    }
    fn measurement_options(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn revisions(&mut self, selection: &RevisionSelection) -> Result<Vec<Commit>, ExperimentError>;
    fn tool(&mut self, revision: &Commit) -> Result<Self::Tool, ExperimentError>;

    /// Release revision-specific files after all measurements, including failed revisions.
    fn finish_revision(&mut self, _revision: &Commit) -> Result<(), ExperimentError> {
        Ok(())
    }
}

pub struct YosysProvider {
    pub git: GitManager,
    pub builder: CcacheCMakeBuilder,
    pub output_dir: PathBuf,
}

impl ToolProvider for YosysProvider {
    type Tool = Yosys;

    fn set_progress(&mut self, progress: crate::progress::Progress) {
        self.builder.set_progress(progress);
    }

    fn repository(&self) -> String {
        self.git.repository_identity()
    }
    fn history(&self) -> String {
        self.git.history_identity().into()
    }
    fn measurement_options(&self) -> serde_json::Value {
        self.builder.measurement_options()
    }

    fn revisions(&mut self, selection: &RevisionSelection) -> Result<Vec<Commit>, ExperimentError> {
        self.builder.progress().stage("Preparing repository");
        self.git.initialize()?;
        self.builder.progress().stage("Reading revision history");
        Ok(self.git.revisions(selection)?)
    }

    fn tool(&mut self, revision: &Commit) -> Result<Yosys, ExperimentError> {
        self.builder.progress().stage("Checking out revision");
        self.git.set_revision(revision.id())?;
        let prefix = self.builder.build_with_logs(
            self.git.directory(),
            &self.output_dir.join("build").join(revision.id().id()),
            &self.output_dir.join("install").join(revision.id().id()),
            &self.output_dir.join("logs").join(revision.id().id()),
        )?;
        let executable = prefix.join("bin/yosys");
        if !executable.is_file() {
            return Err(ExperimentError::Config(format!(
                "missing installed Yosys: {}",
                executable.display()
            )));
        }
        Ok(Yosys::new(executable))
    }

    fn finish_revision(&mut self, revision: &Commit) -> Result<(), ExperimentError> {
        self.builder
            .progress()
            .stage("Pruning build and install directories");
        for directory in ["build", "install"] {
            let path = self.output_dir.join(directory).join(revision.id().id());
            match fs::remove_dir_all(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(io::Error::new(
                        error.kind(),
                        format!("cannot prune {}: {error}", path.display()),
                    )
                    .into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pruning_removes_only_the_finished_revisions_build_and_install() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path();
        for directory in [
            "build/first",
            "install/first",
            "build/next",
            "install/next",
            "logs/first",
            "ccache",
            "source",
        ] {
            let path = output.join(directory);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("keep"), "contents").unwrap();
        }
        let mut provider = YosysProvider {
            git: GitManager::new(output.join("source"), "main", None),
            builder: CcacheCMakeBuilder::new(output.join("ccache")),
            output_dir: output.into(),
        };
        let revision = serde_json::from_value(serde_json::json!({
            "id": "first",
            "committed_at": std::time::UNIX_EPOCH,
            "author": "test",
            "commit_message": "test",
        }))
        .unwrap();
        provider.finish_revision(&revision).unwrap();
        assert!(!output.join("build/first").exists());
        assert!(!output.join("install/first").exists());
        for directory in [
            "build/next",
            "install/next",
            "logs/first",
            "ccache",
            "source",
        ] {
            assert_eq!(
                fs::read_to_string(output.join(directory).join("keep")).unwrap(),
                "contents"
            );
        }
        // A failed build may never have created either directory.
        provider.finish_revision(&revision).unwrap();
    }
}
