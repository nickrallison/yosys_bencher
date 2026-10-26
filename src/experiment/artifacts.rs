use super::{ExperimentError, ProcessCapture};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) struct ArtifactWriter {
    root: PathBuf,
}

impl ArtifactWriter {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn write_files(
        &self,
        directory: &Path,
        files: &BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<(), ExperimentError> {
        if files.is_empty() {
            return Ok(());
        }
        let directory = self.root.join(directory);
        fs::create_dir_all(&directory)?;
        for (name, contents) in files {
            fs::write(directory.join(name), contents)?;
        }
        Ok(())
    }

    pub fn process(
        &self,
        index: usize,
        label: &str,
        captured: &ProcessCapture,
    ) -> Result<(), ExperimentError> {
        let prefix = self.root.join(format!("process-{index:03}-{label}"));
        fs::create_dir_all(&self.root)?;
        fs::write(prefix.with_extension("stdout.log"), &captured.output.stdout)?;
        fs::write(prefix.with_extension("stderr.log"), &captured.output.stderr)?;
        fs::write(prefix.with_extension("command.log"), &captured.command)?;
        if let Some(bytes) = &captured.perf {
            fs::write(prefix.with_extension("perf.json"), bytes)?;
        }
        Ok(())
    }
}
