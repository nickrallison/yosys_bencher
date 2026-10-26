use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Design {
    pub files: Vec<SourceFile>,
    pub headers: Vec<SourceFile>,
    pub options: ReadOptions,
    pub top: Option<String>,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
}

impl SourceFile {
    pub fn new(path: impl Into<PathBuf>, source: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: source.into(),
        }
    }

    pub fn from_path(root: impl AsRef<Path>, path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        Ok(Self::new(
            path,
            std::fs::read_to_string(root.as_ref().join(path))?,
        ))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadOptions {
    pub system_verilog: bool,
    pub defines: BTreeMap<String, Option<String>>,
    pub include_dirs: Vec<PathBuf>,
    pub extra_args: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design() -> Design {
        Design {
            files: vec![
                SourceFile::new("rtl/child.sv", "module child; endmodule"),
                SourceFile::new("rtl/top.sv", "module top; child c(); endmodule"),
            ],
            headers: vec![SourceFile::new("include/config.vh", "`define WIDTH 8")],
            options: ReadOptions {
                system_verilog: true,
                defines: BTreeMap::from([("SYNTHESIS".into(), None)]),
                include_dirs: vec!["include".into()],
                extra_args: vec!["-nolatches".into()],
            },
            top: Some("top".into()),
            parameters: BTreeMap::from([("WIDTH".into(), "16".into())]),
        }
    }

    #[test]
    fn filelist_and_elaboration_settings_survive_serialization() {
        let original = design();
        let restored: Design =
            serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        assert_eq!(restored, original);
        assert_eq!(restored.files[0].path, Path::new("rtl/child.sv"));
        assert_eq!(restored.files[1].path, Path::new("rtl/top.sv"));

        let automatic_top: Design = serde_json::from_str(r#"{"files":[]}"#).unwrap();
        assert!(automatic_top.top.is_none());
    }

    #[test]
    fn source_order_headers_and_options_affect_identity() {
        let original = design();
        let mut reordered = original.clone();
        reordered.files.swap(0, 1);
        assert_ne!(reordered, original);

        let mut changed_header = original.clone();
        changed_header.headers[0].source = "`define WIDTH 32".into();
        assert_ne!(changed_header, original);

        let mut changed_define = original.clone();
        changed_define.options.defines.insert("DEBUG".into(), None);
        assert_ne!(changed_define, original);

        let mut changed_parameter = original.clone();
        changed_parameter
            .parameters
            .insert("WIDTH".into(), "32".into());
        assert_ne!(changed_parameter, original);

        let mut automatic_top = original.clone();
        automatic_top.top = None;
        assert_ne!(automatic_top, original);
    }
}
