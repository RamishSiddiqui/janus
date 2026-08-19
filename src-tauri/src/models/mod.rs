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
use surrealdb::types::{RecordId, RecordIdKey};

/// Extracts a [`RecordIdKey`]'s raw string form. Janus only ever constructs
/// `String`-keyed records (UUIDs or fixed slugs like `"char-aria-silverleaf"`);
/// the other variants (Number/Uuid/Array/Object/Range) never occur in
/// practice, so they fall back to a debug rendering rather than a precise
/// per-variant conversion.
fn record_id_key_to_string(key: &RecordIdKey) -> String {
    match key {
        RecordIdKey::String(s) => s.clone(),
        RecordIdKey::Number(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

/// Serializes a SurrealDB RecordId as just its key string (without table prefix)
pub fn serialize_thing<S>(thing: &RecordId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&record_id_key_to_string(&thing.key))
}

/// Strips SurrealQL backtick-quoting from a record key, if present, and
/// unescapes any `` \` `` / `\\` sequences within — the inverse of
/// `EscapeRecordKey`'s escaping in surrealdb-types (used by `RecordId`'s
/// `ToSql`/`into_json_value()` output). Any key containing a non-alphanumeric,
/// non-underscore character (e.g. every UUID, which contains hyphens) gets
/// backtick-wrapped by that escaping — `RecordId::parse_simple` does a naive
/// `split_once(':')` with no unescaping at all, so every hyphenated ID would
/// otherwise deserialize with the literal backticks still embedded in it.
fn unescape_record_key(raw: &str) -> String {
    if raw.len() >= 2 && raw.starts_with('`') && raw.ends_with('`') {
        let inner = &raw[1..raw.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        raw.to_string()
    }
}

/// Parses a `"table:key"` string (optionally backtick-quoted key) into a
/// `RecordId` — see [`unescape_record_key`] for why this exists instead of
/// `RecordId::parse_simple`.
fn parse_record_id(s: &str) -> Option<RecordId> {
    let (table, key_raw) = s.split_once(':')?;
    Some(RecordId::new(table, unescape_record_key(key_raw)))
}

/// Deserializes a RecordId from the `"table:key"` string the query-result
/// JSON bridge produces (`Value::into_json_value()` renders a `RecordId` as
/// its bare SurrealQL `table:key` form, not a structured object) — see
/// `db::value_bridge`.
pub fn deserialize_thing<'de, D>(deserializer: D) -> Result<RecordId, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_record_id(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid record id: {s}")))
}

/// Serializes an Option<RecordId> as Option<String>
pub fn serialize_option_thing<S>(thing: &Option<RecordId>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match thing {
        Some(t) => serializer.serialize_some(&record_id_key_to_string(&t.key)),
        None => serializer.serialize_none(),
    }
}

/// Deserializes an Option<RecordId> — see [`deserialize_thing`].
pub fn deserialize_option_thing<'de, D>(deserializer: D) -> Result<Option<RecordId>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    s.map(|s| {
        parse_record_id(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid record id: {s}")))
    })
    .transpose()
}

/// Deserializes a datetime field into a plain RFC3339 String. The
/// query-result JSON bridge (`Value::into_json_value()`) already renders
/// SurrealDB's `datetime` type as an RFC3339 string, so this is now just a
/// pass-through — kept as a named helper (rather than switching every model
/// field to plain `String`) so the "this came from a SurrealDB datetime, not
/// an arbitrary string" intent stays visible at each field.
pub fn deserialize_datetime<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)
}

/// [`deserialize_datetime`] for fields like `last_accessed` that are absent
/// until first set, including on rows created before the field existed.
pub fn deserialize_option_datetime<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
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
