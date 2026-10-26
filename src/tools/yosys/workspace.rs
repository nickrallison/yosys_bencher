use super::*;
use std::fs;
use tempfile::TempDir;

#[derive(Debug)]
pub struct PreparedYosysJob {
    pub(super) directory: TempDir,
    pub(super) plan: YosysPlan,
}

impl YosysPlan {
    fn write_to(&self, destination: &Path) -> Result<()> {
        for file in self.files() {
            let target = destination.join(&file.path);
            fs::create_dir_all(target.parent().expect("file has a parent"))?;
            fs::write(target, &file.source)?;
        }
        for directory in self.directories() {
            fs::create_dir_all(destination.join(directory))?;
        }
        stage_includes(destination, self.includes())?;
        fs::create_dir_all(destination.join(".yosys-logs"))?;
        Ok(())
    }
}

impl PreparedYosysJob {
    pub(super) fn materialize(plan: YosysPlan, workspace: &Path) -> Result<Self> {
        fs::create_dir_all(workspace)?;
        let directory = tempfile::Builder::new()
            .prefix(".job-")
            .tempdir_in(workspace)?;
        plan.write_to(directory.path())?;
        Ok(Self { directory, plan })
    }

    pub(super) fn reset_logs(&self) -> Result<()> {
        let directory = self.directory.path().join(".yosys-logs");
        fs::remove_dir_all(&directory)?;
        fs::create_dir(&directory)?;
        Ok(())
    }

    pub(super) fn capture_logs(&self) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        fs::read_dir(self.directory.path().join(".yosys-logs"))?
            .map(|entry| {
                let entry = entry?;
                Ok((PathBuf::from(entry.file_name()), fs::read(entry.path())?))
            })
            .collect()
    }

    pub(super) fn retained_scripts(&self, workspace: &Path) -> PathBuf {
        workspace.join("scripts").join(
            self.directory
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .trim_start_matches('.'),
        )
    }

    pub(super) fn copy_to(&self, destination: &Path) -> Result<()> {
        self.plan.write_to(destination)
    }
}

fn stage_includes(directory: &Path, includes: &[(PathBuf, PathBuf)]) -> Result<()> {
    for (alias, target) in includes {
        let alias = directory.join(alias);
        fs::create_dir_all(alias.parent().unwrap())?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, alias)?;
        #[cfg(not(unix))]
        return Err(ExperimentError::Unsupported(
            "staging include directories requires Unix symlinks".into(),
        ));
    }
    Ok(())
}

impl YosysRunner {
    pub(super) fn netlist(
        &self,
        job: &PreparedYosysJob,
        label: &str,
        context: &mut RunContext,
    ) -> Result<()> {
        check_label(label)?;
        let directory = context.workspace().join("netlists");
        fs::create_dir_all(&directory)?;
        let path = directory.canonicalize()?.join(format!("{label}.v"));
        let script = format!(
            "{}write_verilog {}\n",
            job.plan.script(),
            quote_path(&path)?
        );
        fs::write(job.directory.path().join("inspect.ys"), &script)?;
        if *context.artifacts().scripts() {
            fs::write(
                job.retained_scripts(context.workspace())
                    .join(format!("inspect-{label}.ys")),
                script,
            )?;
        }
        let captured = self.capture(
            job,
            "inspect.ys",
            &format!("inspect-{label}"),
            false,
            context,
        )?;
        captured.validate_stages(&job.plan)?;
        if !path.is_file() {
            return Err(ExperimentError::Parse(format!(
                "missing diagnostic netlist {}",
                path.display()
            )));
        }
        Ok(())
    }
}
