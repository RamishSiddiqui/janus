use std::collections::HashMap;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::MythicError;
use crate::models::provider::ProviderConfig;

/// Row returned from the enabled_models table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnabledModelRow {
    pub provider_id: String,
    pub model_id: String,
    pub model_type: String,
}

/// Internal struct for deserializing enabled_models with the `enabled` flag.
#[derive(Debug, Clone, serde::Deserialize)]
struct EnabledModelFull {
    provider_id: surrealdb::sql::Thing,
    model_id: String,
    model_type: String,
    enabled: bool,
}

pub struct ProviderRepo;

impl ProviderRepo {
    /// Creates a new provider. If `is_default`, unsets other defaults of the same type first.
    pub async fn create(
        db: &Surreal<Db>,
        name: &str,
        provider_type: &str,
        adapter: &str,
        config: serde_json::Value,
        is_default: bool,
    ) -> Result<ProviderConfig, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();

        // If this is set as default, unset any existing default for this type
        if is_default {
            db.query("UPDATE provider_configs SET is_default = false WHERE provider_type = $ptype")
                .bind(("ptype", provider_type.to_string()))
                .await?;
        }

        let mut result = db
            .query(
                "CREATE type::thing('provider_configs', $id) CONTENT {
                    name: $name,
                    provider_type: $ptype,
                    adapter: $adapter,
                    config: $config,
                    is_default: $is_default,
                }",
            )
            .bind(("id", id.clone()))
            .bind(("name", name.to_string()))
            .bind(("ptype", provider_type.to_string()))
            .bind(("adapter", adapter.to_string()))
            .bind(("config", config))
            .bind(("is_default", is_default))
            .await?;

        let created: Option<ProviderConfig> = result.take(0)?;
        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create provider".into()))
    }

    /// Gets a single provider by ID.
    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<ProviderConfig, MythicError> {
        let provider: Option<ProviderConfig> = db.select(("provider_configs", id)).await?;
        provider.ok_or_else(|| MythicError::NotFound(format!("Provider not found: {}", id)))
    }

    /// Lists all providers, optionally filtered by type.
    pub async fn list(
        db: &Surreal<Db>,
        provider_type: Option<&str>,
    ) -> Result<Vec<ProviderConfig>, MythicError> {
        let providers = if let Some(ptype) = provider_type {
            let mut result = db
                .query("SELECT * FROM provider_configs WHERE provider_type = $ptype ORDER BY is_default DESC, name ASC")
                .bind(("ptype", ptype.to_string()))
                .await?;
            let rows: Vec<ProviderConfig> = result.take(0)?;
            rows
        } else {
            let mut result = db
                .query("SELECT * FROM provider_configs ORDER BY provider_type, is_default DESC, name ASC")
                .await?;
            let rows: Vec<ProviderConfig> = result.take(0)?;
            rows
        };
        Ok(providers)
    }

    /// Updates provider name and/or config. Returns the updated provider.
    pub async fn update(
        db: &Surreal<Db>,
        id: &str,
        name: Option<&str>,
        config: Option<serde_json::Value>,
    ) -> Result<ProviderConfig, MythicError> {
        // Verify exists first
        Self::get(db, id).await?;

        let mut sets = Vec::new();
        let mut bindings = serde_json::Map::new();

        if let Some(name) = name {
            sets.push("name = $name");
            bindings.insert("name".into(), serde_json::Value::String(name.to_string()));
        }
        if let Some(config) = config {
            sets.push("config = $config");
            bindings.insert("config".into(), config);
        }

        if sets.is_empty() {
            return Self::get(db, id).await;
        }

        let query = format!(
            "UPDATE type::thing('provider_configs', $id) SET {}",
            sets.join(", ")
        );
        bindings.insert("id".into(), serde_json::Value::String(id.to_string()));

        let mut result = db
            .query(&query)
            .bind(serde_json::Value::Object(bindings))
            .await?;

        let updated: Option<ProviderConfig> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Provider not found: {}", id)))
    }

    /// Deletes a provider by ID.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        let result: Option<ProviderConfig> = db.delete(("provider_configs", id)).await?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!("Provider not found: {}", id)));
        }
        Ok(())
    }

    /// Sets a provider as default for its type, unsetting all others of the same type.
    pub async fn set_default(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        // Get the provider to find its type
        let provider = Self::get(db, id).await?;
        let ptype = serde_json::to_value(&provider.provider_type)
            .map_err(|e| MythicError::DatabaseOp(format!("Failed to serialize provider type: {}", e)))?;
        let ptype_str = ptype.as_str().unwrap_or("llm");

        // Unset all defaults for this type
        db.query("UPDATE provider_configs SET is_default = false WHERE provider_type = $ptype")
            .bind(("ptype", ptype_str.to_string()))
            .await?;

        // Set this one as default
        db.query("UPDATE type::thing('provider_configs', $id) SET is_default = true")
            .bind(("id", id.to_string()))
            .await?;

        Ok(())
    }

    /// Gets the default provider for a given type.
    pub async fn get_default(
        db: &Surreal<Db>,
        provider_type: &str,
    ) -> Result<Option<ProviderConfig>, MythicError> {
        let mut result = db
            .query("SELECT * FROM provider_configs WHERE provider_type = $ptype ORDER BY is_default DESC, name ASC LIMIT 1")
            .bind(("ptype", provider_type.to_string()))
            .await?;
        let providers: Vec<ProviderConfig> = result.take(0)?;
        Ok(providers.into_iter().next())
    }

    // ── Enabled Models ───────────────────────────────────────────────────

    /// Toggles a model's enabled state using UPSERT with a deterministic composite ID.
    pub async fn toggle_model(
        db: &Surreal<Db>,
        provider_id: &str,
        model_id: &str,
        model_type: &str,
        enabled: bool,
    ) -> Result<(), MythicError> {
        // Build a deterministic composite ID from provider_id + model_id
        let composite_id = format!(
            "{}_{}",
            provider_id,
            model_id.replace('/', "_").replace(':', "_").replace('.', "_")
        );

        db.query(
            "UPSERT type::thing('enabled_models', $composite_id) CONTENT {
                provider_id: type::thing('provider_configs', $provider_id),
                model_id: $model_id,
                model_type: $model_type,
                enabled: $enabled,
                updated_at: time::now(),
            }",
        )
        .bind(("composite_id", composite_id))
        .bind(("provider_id", provider_id.to_string()))
        .bind(("model_id", model_id.to_string()))
        .bind(("model_type", model_type.to_string()))
        .bind(("enabled", enabled))
        .await?;

        Ok(())
    }

    /// Lists enabled model entries (enabled=true), optionally filtered by provider.
    pub async fn list_enabled_models(
        db: &Surreal<Db>,
        provider_id: Option<&str>,
    ) -> Result<Vec<EnabledModelRow>, MythicError> {
        let rows: Vec<EnabledModelFull> = if let Some(pid) = provider_id {
            let mut result = db
                .query("SELECT * FROM enabled_models WHERE enabled = true AND provider_id = type::thing('provider_configs', $pid)")
                .bind(("pid", pid.to_string()))
                .await?;
            result.take(0)?
        } else {
            let mut result = db
                .query("SELECT * FROM enabled_models WHERE enabled = true")
                .await?;
            result.take(0)?
        };

        Ok(rows
            .into_iter()
            .map(|r| EnabledModelRow {
                provider_id: r.provider_id.id.to_raw(),
                model_id: r.model_id,
                model_type: r.model_type,
            })
            .collect())
    }

    /// Gets all enabled model states as a lookup map: (provider_id, model_id) → enabled.
    /// Used by `list_all_models` to merge with HTTP-fetched model lists.
    pub async fn get_all_enabled_states(
        db: &Surreal<Db>,
    ) -> Result<HashMap<(String, String), bool>, MythicError> {
        let mut result = db
            .query("SELECT * FROM enabled_models")
            .await?;
        let rows: Vec<EnabledModelFull> = result.take(0).unwrap_or_default();

        Ok(rows
            .into_iter()
            .map(|r| ((r.provider_id.id.to_raw(), r.model_id), r.enabled))
            .collect())
    }
}
