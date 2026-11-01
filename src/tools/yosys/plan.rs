use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Getters)]
#[getset(get = "pub")]
pub struct YosysPlan {
    script: String,
    files: Vec<SourceFile>,
    directories: Vec<PathBuf>,
    includes: Vec<(PathBuf, PathBuf)>,
    stages: Vec<String>,
}

impl YosysJob {
    pub fn compile(&self) -> Result<YosysPlan> {
        let job = self;
        let mut files = Vec::new();
        let mut directories = Vec::new();
        let mut seen = HashSet::from([PathBuf::from("run.ys"), PathBuf::from("inspect.ys")]);
        for file in job
            .design
            .files
            .iter()
            .chain(&job.design.headers)
            .chain(&job.assets)
        {
            let path = relative_path(&file.path, false)?;
            if path.starts_with(".yosys-includes")
                || path.starts_with(".yosys-logs")
                || !seen.insert(path.clone())
            {
                return Err(ExperimentError::Config(format!(
                    "conflicting job path {}",
                    path.display()
                )));
            }
            files.push(SourceFile::new(path, file.source.clone()));
        }
        let mut includes = Vec::new();
        for (index, path) in job.design.options.include_dirs.iter().enumerate() {
            let path = relative_path(path, true)?;
            if path.starts_with(".yosys-includes") || path.starts_with(".yosys-logs") {
                return Err(ExperimentError::Config(format!(
                    "reserved include path {}",
                    path.display()
                )));
            }
            directories.push(path.clone());
            let alias = PathBuf::from(format!(".yosys-includes/{index}"));
            let target = PathBuf::from("..").join(path);
            includes.push((alias, target));
        }
        let mut commands = frontend_commands(&job.design)?;
        let mut stages = HashSet::new();
        for stage in &job.stages {
            check_label(&stage.name)?;
            if stage.commands.is_empty() || !stages.insert(&stage.name) {
                return Err(ExperimentError::Config(format!(
                    "invalid or duplicate stage {}",
                    stage.name
                )));
            }
            for (index, command) in stage.commands.iter().enumerate() {
                let mode = if index == 0 { "-o" } else { "-a" };
                commands.push(format!(
                    "tee {mode} .yosys-logs/{}.log {command}",
                    stage.name
                ));
            }
        }
        let script = format!("{}\n", commands.join("\n"));
        files.push(SourceFile::new("run.ys", script.clone()));
        for file in &files {
            for ancestor in file.path.ancestors().skip(1) {
                if seen.contains(ancestor) {
                    return Err(ExperimentError::Config(format!(
                        "file is also a directory: {}",
                        ancestor.display()
                    )));
                }
            }
        }
        for directory in &directories {
            if directory.ancestors().any(|path| seen.contains(path)) {
                return Err(ExperimentError::Config(format!(
                    "include path conflicts with a file: {}",
                    directory.display()
                )));
            }
        }
        Ok(YosysPlan {
            script,
            files,
            directories,
            includes,
            stages: job.stages.iter().map(|stage| stage.name.clone()).collect(),
        })
    }
}
