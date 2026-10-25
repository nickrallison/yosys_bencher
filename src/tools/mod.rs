pub mod perf;
pub mod yosys;

pub(crate) fn executable(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    if path.components().count() == 1
        && !path.is_absolute()
        && let Some(paths) = std::env::var_os("PATH")
    {
        for directory in std::env::split_paths(&paths) {
            let candidate = directory.join(path);
            if candidate.is_file() {
                return candidate.canonicalize();
            }
        }
    }
    path.canonicalize()
}
