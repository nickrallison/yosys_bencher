#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{field}: {message}")]
    Validation { field: String, message: String },
    #[error("{0}")]
    Invalid(String),
    #[error("unsupported stored procedure or schema: {0}")]
    Unsupported(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("analysis expired because the database changed; refresh the result")]
    Expired,
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, StoreError>;

impl From<&StoreError> for yosys_bencher_ui_types::api::ApiError {
    fn from(error: &StoreError) -> Self {
        use yosys_bencher_ui_types::api::ErrorKind;
        let (kind, field) = match error {
            StoreError::Validation { field, .. } => (ErrorKind::Validation, Some(field.clone())),
            StoreError::Invalid(_) => (ErrorKind::Validation, None),
            StoreError::NotFound(_) => (ErrorKind::NotFound, None),
            StoreError::Unsupported(_) => (ErrorKind::Unsupported, None),
            StoreError::Expired => (ErrorKind::Expired, None),
            _ => (ErrorKind::Storage, None),
        };
        Self {
            kind,
            field,
            message: error.to_string(),
        }
    }
}
