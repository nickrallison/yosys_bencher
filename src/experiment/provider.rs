use std::path::PathBuf;

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
        let prefix = self.builder.build(
            self.git.directory(),
            &self.output_dir.join("build").join(revision.id().id()),
            &self.output_dir.join("install").join(revision.id().id()),
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
}
