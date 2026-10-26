#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub fn required<T>(result: Result<T, impl std::fmt::Display>) -> T {
    result.unwrap_or_else(|error| {
        log::error!("{error}");
        panic!("{error}");
    })
}

pub fn tool(name: &str, override_env: Option<&str>) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = PathBuf::from(
        override_env
            .and_then(std::env::var_os)
            .unwrap_or_else(|| name.into()),
    );
    let candidates = if path.components().count() == 1 {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(&path))
            .collect()
    } else {
        vec![path.clone()]
    };
    required(
        candidates
            .into_iter()
            .find(|candidate| {
                candidate.metadata().is_ok_and(|metadata| {
                    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                })
            })
            .map(|candidate| required(candidate.canonicalize()))
            .ok_or_else(|| {
                let hint = override_env
                    .map(|variable| format!(" or set {variable}"))
                    .unwrap_or_default();
                format!("missing test tool {}; install {name}{hint}", path.display())
            }),
    )
}

pub fn file(variable: &str) -> PathBuf {
    let path =
        PathBuf::from(required(std::env::var_os(variable).ok_or_else(|| {
            format!("set {variable} to the required test input file")
        })));
    required(
        path.canonicalize()
            .map_err(|error| format!("{variable}={}: {error}", path.display())),
    )
}

pub fn output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
    use std::os::unix::process::ExitStatusExt;
    Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    }
}

pub struct Repository {
    path: PathBuf,
}

impl Repository {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let repository = Self { path: path.into() };
        fs::create_dir_all(&repository.path).unwrap();
        repository.git(&["init", "-b", "main"]);
        repository
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn commit(&self, files: &[(&str, &str)]) -> String {
        for (name, contents) in files {
            fs::write(self.path.join(name), contents).unwrap();
        }
        self.git(&["add", "."]);
        self.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "fixture",
        ]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new(tool("git", None))
            .arg("-C")
            .arg(&self.path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().into()
    }
}
