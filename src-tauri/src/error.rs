use thiserror::Error;

/// Unified error type for the Mythic application.
///
/// All backend errors are funneled through this enum so that
/// Tauri command handlers can return a single, serializable error type.
#[derive(Debug, Error)]
pub enum MythicError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Database migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Cancelled")]
    Cancelled,
}

/// Validates that a string field doesn't exceed the maximum allowed length.
/// Returns `Ok(())` if valid, or `Err(MythicError::Validation(...))` if too long.
pub fn validate_string_length(field: &str, value: &str, max_len: usize) -> Result<(), MythicError> {
    if value.len() > max_len {
        Err(MythicError::Validation(format!(
            "{} is too long ({} chars, max {})",
            field, value.len(), max_len
        )))
    } else {
        Ok(())
    }
}

/// Validates that a required string field is not empty and not too long.
pub fn validate_required_string(field: &str, value: &str, max_len: usize) -> Result<(), MythicError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MythicError::Validation(format!("{} cannot be empty", field)));
    }
    validate_string_length(field, trimmed, max_len)
}

/// Make MythicError serializable for Tauri IPC.
///
/// Tauri requires command errors to implement `Into<tauri::ipc::InvokeError>`.
/// We serialize to a JSON object with `error` and `message` fields.
impl serde::Serialize for MythicError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("MythicError", 2)?;

        let variant = match self {
            MythicError::Database(_) => "database",
            MythicError::Migration(_) => "migration",
            MythicError::Http(_) => "http",
            MythicError::Serialization(_) => "serialization",
            MythicError::Io(_) => "io",
            MythicError::Image(_) => "image",
            MythicError::Provider(_) => "provider",
            MythicError::Validation(_) => "validation",
            MythicError::NotFound(_) => "not_found",
            MythicError::Config(_) => "config",
            MythicError::Cancelled => "cancelled",
        };

        state.serialize_field("error", variant)?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}
