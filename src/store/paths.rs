use super::*;
use std::path::Component;

// Missing artifacts must remain representable, so normalize without filesystem access.
pub(super) fn stored(path: &Path, base: Option<&Path>) -> Result<PathBuf> {
    let mut absolute = PathBuf::new();
    for part in std::path::absolute(path)?.components() {
        match part {
            Component::ParentDir => {
                absolute.pop();
            }
            Component::CurDir => {}
            _ => absolute.push(part.as_os_str()),
        }
    }
    let Some(base) = base else {
        return Ok(absolute);
    };
    let target: Vec<_> = absolute.components().collect();
    let source: Vec<_> = base.components().collect();
    let common = target
        .iter()
        .zip(&source)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return Err(invalid(
            "Artifacts and database must be on the same filesystem root",
        ));
    }
    let mut relative = PathBuf::new();
    for _ in &source[common..] {
        relative.push("..");
    }
    for part in &target[common..] {
        relative.push(part.as_os_str());
    }
    Ok(relative)
}
