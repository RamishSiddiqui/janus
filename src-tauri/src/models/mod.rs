pub mod ai_horde_model;
pub mod character;
pub mod conversation;
pub mod conversation_character;
pub mod image_preset;
pub mod lorebook;
pub mod memory;
pub mod npc_candidate;
pub mod persona;
pub mod provider;
pub mod scene;
pub mod scene_state;
pub mod summary;

use serde::{Deserialize, Deserializer, Serializer};
use surrealdb::sql::Thing;

/// Serializes a SurrealDB Thing as just its ID string (without table prefix)
pub fn serialize_thing<S>(thing: &Thing, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&thing.id.to_raw())
}

/// Deserializes a Thing from SurrealDB responses
pub fn deserialize_thing<'de, D>(deserializer: D) -> Result<Thing, D::Error>
where
    D: Deserializer<'de>,
{
    Thing::deserialize(deserializer)
}

/// Serializes an Option<Thing> as Option<String>
pub fn serialize_option_thing<S>(thing: &Option<Thing>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match thing {
        Some(t) => serializer.serialize_some(&t.id.to_raw()),
        None => serializer.serialize_none(),
    }
}

/// Deserializes an Option<Thing> from SurrealDB responses
pub fn deserialize_option_thing<'de, D>(deserializer: D) -> Result<Option<Thing>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<Thing>::deserialize(deserializer)
}

/// Deserializes a SurrealDB Datetime into a plain RFC3339 String — `.to_raw()`,
/// not `.to_string()`/`Display`, which wraps the value in SurrealQL literal
/// syntax (`d'2024-01-01T00:00:00Z'`, quotes and `d` prefix included) that
/// JS's `new Date(...)` can't parse and silently turns into "NaNd ago"
/// throughout the frontend.
pub fn deserialize_datetime<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let dt = surrealdb::sql::Datetime::deserialize(deserializer)?;
    Ok(dt.to_raw())
}

/// Deserializes an optional SurrealDB Datetime into an Option<String> — for
/// fields like `last_accessed` that are absent until first set, including on
/// rows created before the field existed. See [`deserialize_datetime`] for
/// why `.to_raw()` and not `.to_string()`.
pub fn deserialize_option_datetime<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let dt = Option::<surrealdb::sql::Datetime>::deserialize(deserializer)?;
    Ok(dt.map(|d| d.to_raw()))
}

/// A minimal recursive JSON type used purely as a `#[specta(type = JsonValue)]`
/// override for `serde_json::Value` fields (arbitrary-shape blobs like
/// character card data, provider config, message metadata).
///
/// specta's own built-in `serde_json::Value` support (`legacy_impls.rs`,
/// `serde_json` feature) registers it as an *inline* type whose `Array`/
/// `Object` variants re-expand `Value::definition()` with no cycle-breaking —
/// genuine infinite recursion on export, unrelated to any real data depth.
/// A normally-derived enum like this one IS registered as a named/cached
/// type by the derive macro, so the recursion terminates correctly. The
/// actual Rust field stays `serde_json::Value`; this type only stands in for
/// specta's reflection.
///
/// `#[serde(untagged)]` is required so the exported TS type matches the real
/// wire shape: `serde_json::Value` serializes as plain JSON (a bare number,
/// string, object, etc.), not as a `{ "Bool": true }`-style tagged variant.
/// Without it, specta falls back to externally-tagged rendering, producing a
/// TS type that looks plausible but never matches the actual payload. The
/// `Serialize`/`Deserialize` derives are never used to move real data (this
/// type is never constructed) — they exist only so `#[serde(untagged)]` is a
/// legal attribute here for specta's own (derive-independent) attribute
/// parsing to read.
#[derive(specta::Type, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(std::collections::HashMap<String, JsonValue>),
}

/// Wraps `serde_json::Value` for use as a `#[tauri::command]` parameter or
/// return type. Rust doesn't support a per-parameter `#[specta(type = ...)]`
/// override (unlike struct fields), so bare `serde_json::Value` parameters
/// hit the same infinite-recursion bug `JsonValue` exists to avoid — this
/// newtype is the parameter-position equivalent. `#[serde(transparent)]`
/// means it serializes identically to the bare `serde_json::Value` on the
/// wire; only the specta-facing type differs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct DynamicJson(pub serde_json::Value);

impl specta::Type for DynamicJson {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        JsonValue::definition(types)
    }
}
