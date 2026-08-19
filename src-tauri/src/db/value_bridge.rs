//! Bridges Janus's existing serde-based model structs to surrealdb 3.x's
//! new `SurrealValue`-based query API.
//!
//! surrealdb 3.x replaced the old serde-`Deserialize`-based `.take()` (and
//! the `.select()`/`.create()`/`.delete()` shorthand methods) with a bound
//! on its own `SurrealValue` trait — not implementable via a simple derive
//! for structs with custom `RecordId`/free-form-JSON field handling like
//! Janus's. Rather than hand-writing `SurrealValue` for every model (and
//! losing the existing serde `Thing`-serialization helpers in
//! `models::mod`), every DB call site pins its generic parameter to
//! `surrealdb::types::Value` (which trivially satisfies `SurrealValue`,
//! being the identity case) and converts through JSON using these helpers —
//! `Value::into_json_value()` for reads (a total, never-fails conversion
//! surrealdb itself provides) and [`to_surreal_value`] for writes (no
//! built-in reverse bridge exists, so this is a small hand-written mapping
//! over the JSON subset Janus actually sends: object/array/string/number/
//! bool/null).

use serde::de::DeserializeOwned;
use surrealdb::types::{Array, Number, Object, RecordId, RecordIdKey, Value};

use crate::error::MythicError;

/// Extracts a [`RecordId`]'s raw key string — the replacement for the old
/// `surrealdb::sql::Thing`'s `.id.to_raw()` (Thing had `.tb` + `.id: sql::Id`;
/// RecordId has `.table` + `.key: RecordIdKey` instead, no `.id` field at
/// all). Janus only ever constructs `String`-keyed records (UUIDs or fixed
/// slugs like `"char-aria-silverleaf"`); the other variants (Number/Uuid/
/// Array/Object/Range) never occur in practice, so they fall back to a debug
/// rendering rather than a precise per-variant conversion.
pub fn record_id_to_string(id: &RecordId) -> String {
    record_id_key_to_string(&id.key)
}

fn record_id_key_to_string(key: &RecordIdKey) -> String {
    match key {
        RecordIdKey::String(s) => s.clone(),
        RecordIdKey::Number(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

/// Converts a single `Value` (from `.take::<Value>(n)` / `.select::<Option<Value>>()`
/// / etc.) into a typed Rust value via its existing `serde::Deserialize` impl.
pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, MythicError> {
    serde_json::from_value(value.into_json_value()).map_err(MythicError::from)
}

/// [`from_value`] for an `Option<Value>` result (e.g. `.select()`/`.create()`
/// on a single record, or `.take::<Option<Value>>(n)`).
pub fn from_value_opt<T: DeserializeOwned>(value: Option<Value>) -> Result<Option<T>, MythicError> {
    value.map(from_value).transpose()
}

/// [`from_value`] for a `Vec<Value>` result (e.g. `SELECT * FROM table`).
pub fn from_value_vec<T: DeserializeOwned>(values: Vec<Value>) -> Result<Vec<T>, MythicError> {
    values.into_iter().map(from_value).collect()
}

/// Converts a `serde_json::Value` into a surrealdb `Value`, for use as query
/// bind variables (`.bind()`) or `CREATE`/`UPDATE` content (`.content()`) —
/// both now require `impl SurrealValue` rather than accepting a raw JSON
/// blob directly. Handles the plain-JSON subset (object/array/string/
/// number/bool/null); Janus never needs to construct the richer surrealdb-
/// specific variants (Datetime/Duration/RecordId/...) through this path —
/// those are always produced server-side via `type::record()`/`time::now()`
/// casts in the query text itself.
pub fn to_surreal_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => Value::Number(if let Some(i) = n.as_i64() {
            Number::Int(i)
        } else {
            Number::Float(n.as_f64().unwrap_or(0.0))
        }),
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => {
            let values: Vec<Value> = arr.into_iter().map(to_surreal_value).collect();
            Value::Array(Array::from(values))
        }
        serde_json::Value::Object(obj) => {
            let map: std::collections::BTreeMap<String, Value> = obj
                .into_iter()
                .map(|(k, v)| (k, to_surreal_value(v)))
                .collect();
            Value::Object(Object::from(map))
        }
    }
}
