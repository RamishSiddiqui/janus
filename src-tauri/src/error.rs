use thiserror::Error;

/// Unified error type for the Mythic application.
///
/// All backend errors are funneled through this enum so that
/// Tauri command handlers can return a single, serializable error type.
#[derive(Debug, Error)]
pub enum MythicError {
    #[error("Database error: {0}")]
    Database(#[from] surrealdb::Error),

    #[error("Database operation failed: {0}")]
    DatabaseOp(String),

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

/// Retries a fallible SurrealDB operation when it reports a retryable
/// read/write transaction conflict — SurrealDB's RocksDB engine throws
/// "Failed to commit transaction due to a read or write conflict. This
/// transaction can be retried" when two transactions touch overlapping
/// keys concurrently. Observed on cascade-heavy deletes (a conversation
/// delete's cascade event touches many tables) when the user deletes
/// several rows in quick succession — without this, the second delete
/// silently failed with no retry and no user-visible error beyond a log
/// line, leaving the row (and everything it referenced) still in place.
/// Retries up to 2 extra times with a short backoff; any other error, or
/// exhausting the retries, returns immediately.
pub async fn retry_on_conflict<T, F, Fut>(mut op: F) -> Result<T, surrealdb::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, surrealdb::Error>>,
{
    let mut attempt = 0u32;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let retryable = e.to_string().contains("can be retried");
                if retryable && attempt < 2 {
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(120 * attempt as u64))
                        .await;
                    continue;
                }
                return Err(e);
            }
        }
    }
}

/// Validates that a string field doesn't exceed the maximum allowed length.
/// Returns `Ok(())` if valid, or `Err(MythicError::Validation(...))` if too long.
pub fn validate_string_length(field: &str, value: &str, max_len: usize) -> Result<(), MythicError> {
    if value.len() > max_len {
        Err(MythicError::Validation(format!(
            "{} is too long ({} chars, max {})",
            field,
            value.len(),
            max_len
        )))
    } else {
        Ok(())
    }
}

/// Validates that a required string field is not empty and not too long.
pub fn validate_required_string(
    field: &str,
    value: &str,
    max_len: usize,
) -> Result<(), MythicError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(MythicError::Validation(format!(
            "{} cannot be empty",
            field
        )));
    }
    validate_string_length(field, trimmed, max_len)
}

/// Joins a caller-supplied relative path onto `base`, rejecting any path that
/// could escape it — `..` traversal, absolute paths/drive prefixes, or (once
/// the target exists) a symlink that resolves outside `base`.
///
/// Callers that don't require the target to already exist should still check
/// `.exists()` afterward; the symlink check only applies once canonicalization
/// succeeds, which requires the path to exist.
pub fn resolve_within(
    base: &std::path::Path,
    relative: &str,
) -> Result<std::path::PathBuf, MythicError> {
    use std::path::Component;

    if relative.trim().is_empty() {
        return Err(MythicError::Validation("Path cannot be empty".to_string()));
    }

    for component in std::path::Path::new(relative).components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(MythicError::Validation(format!(
                "Invalid path: '{}'",
                relative
            )));
        }
    }

    let joined = base.join(relative);

    if let (Ok(canonical_base), Ok(canonical_joined)) = (base.canonicalize(), joined.canonicalize())
    {
        if !canonical_joined.starts_with(&canonical_base) {
            return Err(MythicError::Validation(format!(
                "Invalid path: '{}'",
                relative
            )));
        }
    }

    Ok(joined)
}

/// Truncates `s` to at most `max_bytes` bytes, backing off to the nearest
/// preceding UTF-8 character boundary rather than potentially landing
/// mid-character. Plain byte-index slicing (`&s[..max_bytes]`) panics the
/// instant `max_bytes` falls inside a multi-byte character — routine in
/// roleplay prose full of em dashes, curly quotes, accented names, and
/// emoji — and with this crate's release profile set to `panic = "abort"`,
/// that panic takes down the whole process, not just the calling task.
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Same as [`truncate_at_char_boundary`] but keeps the *last* `max_bytes`
/// bytes instead of the first — for "most recent N characters of context"
/// truncation, where landing mid-character at the start of the slice would
/// panic just as readily as at the end.
pub fn truncate_tail_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
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
        // Log every IPC error so it always appears in backend traces
        tracing::warn!("[IPC_ERROR] {}", self);

        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("MythicError", 2)?;

        let variant = match self {
            MythicError::Database(_) => "database",
            MythicError::DatabaseOp(_) => "database_op",
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

/// Mirrors the exact `{ error, message }` wire shape produced by
/// `MythicError`'s custom `Serialize` impl above — exists solely so
/// `MythicError` can implement `specta::Type` by delegating to it.
///
/// Every Tauri command returns `Result<T, MythicError>`; specta's blanket
/// impl for async function results requires the whole `Result<T, E>` to
/// implement `Type`, which in turn requires *both* T and E to implement it.
/// Without this, every async command fails to satisfy `FunctionResult` —
/// a runtime-of-macro-expansion error, not something `cargo check` on the
/// type alone would reveal, since compiling this file in isolation is fine.
#[derive(serde::Serialize, specta::Type)]
#[serde(rename = "MythicError")]
struct MythicErrorShape {
    error: String,
    message: String,
}

impl specta::Type for MythicError {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        MythicErrorShape::definition(types)
    }
}
