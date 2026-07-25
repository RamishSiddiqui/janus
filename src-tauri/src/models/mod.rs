pub mod character;
pub mod conversation;
pub mod conversation_character;
pub mod lorebook;
pub mod memory;
pub mod provider;
pub mod scene;
pub mod scene_state;
pub mod summary;

use serde::{Deserializer, Deserialize, Serializer};
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

/// Deserializes a SurrealDB Datetime into a String
pub fn deserialize_datetime<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let dt = surrealdb::sql::Datetime::deserialize(deserializer)?;
    Ok(dt.to_string())
}

/// Deserializes an optional SurrealDB Datetime into an Option<String> — for
/// fields like `last_accessed` that are absent until first set, including on
/// rows created before the field existed.
pub fn deserialize_option_datetime<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let dt = Option::<surrealdb::sql::Datetime>::deserialize(deserializer)?;
    Ok(dt.map(|d| d.to_string()))
}

