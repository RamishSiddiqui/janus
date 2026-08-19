use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::error::MythicError;
use crate::models::image_preset::ImagePreset;

/// Minimal shape for pulling just a conversation's chosen preset reference
/// without deserializing the whole `Conversation`.
#[derive(Debug, serde::Deserialize)]
struct ConvPresetRef {
    #[serde(default, deserialize_with = "crate::models::deserialize_option_thing")]
    image_preset_id: Option<surrealdb::types::RecordId>,
}

pub struct ImagePresetRepo;

impl ImagePresetRepo {
    /// Creates a new preset. If `is_default`, unsets any existing default first.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        db: &Surreal<Db>,
        name: &str,
        model: Option<&str>,
        sampler_name: &str,
        cfg_scale: f64,
        steps: u32,
        karras: bool,
        style: Option<&str>,
        negative_prompt: Option<&str>,
        is_default: bool,
        clip_skip: Option<u32>,
        post_processing: &[String],
        hires_fix: bool,
        hires_fix_denoising_strength: Option<f64>,
    ) -> Result<ImagePreset, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();

        if is_default {
            db.query("UPDATE image_presets SET is_default = false")
                .await?;
        }

        let mut result = db
            .query(
                "CREATE type::record('image_presets', $id) CONTENT {
                    name: $name,
                    model: $model,
                    sampler_name: $sampler_name,
                    cfg_scale: $cfg_scale,
                    steps: $steps,
                    karras: $karras,
                    style: $style,
                    negative_prompt: $negative_prompt,
                    is_default: $is_default,
                    clip_skip: $clip_skip,
                    post_processing: $post_processing,
                    hires_fix: $hires_fix,
                    hires_fix_denoising_strength: $hires_fix_denoising_strength,
                }",
            )
            .bind(("id", id))
            .bind(("name", name.to_string()))
            .bind(("model", model.map(|s| s.to_string())))
            .bind(("sampler_name", sampler_name.to_string()))
            .bind(("cfg_scale", cfg_scale))
            .bind(("steps", steps))
            .bind(("karras", karras))
            .bind(("style", style.map(|s| s.to_string())))
            .bind(("negative_prompt", negative_prompt.map(|s| s.to_string())))
            .bind(("is_default", is_default))
            .bind(("clip_skip", clip_skip))
            .bind(("post_processing", post_processing.to_vec()))
            .bind(("hires_fix", hires_fix))
            .bind(("hires_fix_denoising_strength", hires_fix_denoising_strength))
            .await?;

        let created: Option<ImagePreset> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create image preset".into()))
    }

    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<ImagePreset, MythicError> {
        let preset: Option<ImagePreset> =
            crate::db::value_bridge::from_value_opt(db.select(("image_presets", id)).await?)?;
        preset.ok_or_else(|| MythicError::NotFound(format!("Image preset not found: {}", id)))
    }

    pub async fn list(db: &Surreal<Db>) -> Result<Vec<ImagePreset>, MythicError> {
        let mut result = db
            .query("SELECT * FROM image_presets ORDER BY is_default DESC, name ASC")
            .await?;
        let rows: Vec<ImagePreset> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(rows)
    }

    /// Updates any subset of a preset's fields. Returns the updated preset.
    #[allow(clippy::too_many_arguments)]
    /// Updates any subset of a preset's fields — `None` (unsent) means "leave
    /// as-is"; for `model`/`style`/`negative_prompt`, an empty string means
    /// "clear to unset" (matches the Providers page's plain-string-field
    /// convention, avoiding a nested `Option<Option<T>>` that TypeScript's
    /// generated bindings can't represent distinctly from "don't touch").
    pub async fn update(
        db: &Surreal<Db>,
        id: &str,
        name: Option<&str>,
        model: Option<&str>,
        sampler_name: Option<&str>,
        cfg_scale: Option<f64>,
        steps: Option<u32>,
        karras: Option<bool>,
        style: Option<&str>,
        negative_prompt: Option<&str>,
        // 0 clears to "no override" (valid clip_skip is 1-12), matching the
        // empty-string-means-clear convention used for the string fields above.
        clip_skip: Option<u32>,
        post_processing: Option<&[String]>,
        hires_fix: Option<bool>,
        hires_fix_denoising_strength: Option<f64>,
    ) -> Result<ImagePreset, MythicError> {
        Self::get(db, id).await?;

        let mut sets = Vec::new();
        let mut bindings = serde_json::Map::new();

        fn optional_string(s: &str) -> serde_json::Value {
            if s.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(s.to_string())
            }
        }

        if let Some(name) = name {
            sets.push("name = $name");
            bindings.insert("name".into(), serde_json::Value::String(name.to_string()));
        }
        if let Some(model) = model {
            sets.push("model = $model");
            bindings.insert("model".into(), optional_string(model));
        }
        if let Some(sampler_name) = sampler_name {
            sets.push("sampler_name = $sampler_name");
            bindings.insert(
                "sampler_name".into(),
                serde_json::Value::String(sampler_name.to_string()),
            );
        }
        if let Some(cfg_scale) = cfg_scale {
            sets.push("cfg_scale = $cfg_scale");
            bindings.insert("cfg_scale".into(), serde_json::json!(cfg_scale));
        }
        if let Some(steps) = steps {
            sets.push("steps = $steps");
            bindings.insert("steps".into(), serde_json::json!(steps));
        }
        if let Some(karras) = karras {
            sets.push("karras = $karras");
            bindings.insert("karras".into(), serde_json::Value::Bool(karras));
        }
        if let Some(style) = style {
            sets.push("style = $style");
            bindings.insert("style".into(), optional_string(style));
        }
        if let Some(negative_prompt) = negative_prompt {
            sets.push("negative_prompt = $negative_prompt");
            bindings.insert("negative_prompt".into(), optional_string(negative_prompt));
        }
        if let Some(clip_skip) = clip_skip {
            sets.push("clip_skip = $clip_skip");
            bindings.insert(
                "clip_skip".into(),
                if clip_skip == 0 {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(clip_skip)
                },
            );
        }
        if let Some(post_processing) = post_processing {
            sets.push("post_processing = $post_processing");
            bindings.insert("post_processing".into(), serde_json::json!(post_processing));
        }
        if let Some(hires_fix) = hires_fix {
            sets.push("hires_fix = $hires_fix");
            bindings.insert("hires_fix".into(), serde_json::Value::Bool(hires_fix));
        }
        if let Some(hires_fix_denoising_strength) = hires_fix_denoising_strength {
            sets.push("hires_fix_denoising_strength = $hires_fix_denoising_strength");
            bindings.insert(
                "hires_fix_denoising_strength".into(),
                serde_json::json!(hires_fix_denoising_strength),
            );
        }

        if sets.is_empty() {
            return Self::get(db, id).await;
        }

        let query = format!(
            "UPDATE type::record('image_presets', $id) SET {}",
            sets.join(", ")
        );
        bindings.insert("id".into(), serde_json::Value::String(id.to_string()));

        let mut result = db
            .query(&query)
            .bind(crate::db::value_bridge::to_surreal_value(
                serde_json::Value::Object(bindings),
            ))
            .await?;

        let updated: Option<ImagePreset> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Image preset not found: {}", id)))
    }

    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        let result: Option<ImagePreset> =
            crate::db::value_bridge::from_value_opt(db.delete(("image_presets", id)).await?)?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!(
                "Image preset not found: {}",
                id
            )));
        }
        Ok(())
    }

    /// Sets a preset as the global default, unsetting all others.
    pub async fn set_default(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        Self::get(db, id).await?;

        db.query("UPDATE image_presets SET is_default = false")
            .await?;
        db.query("UPDATE type::record('image_presets', $id) SET is_default = true")
            .bind(("id", id.to_string()))
            .await?;

        Ok(())
    }

    /// Gets the global default preset, if one is set.
    pub async fn get_default(db: &Surreal<Db>) -> Result<Option<ImagePreset>, MythicError> {
        let mut result = db
            .query("SELECT * FROM image_presets WHERE is_default = true LIMIT 1")
            .await?;
        let rows: Vec<ImagePreset> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(rows.into_iter().next())
    }

    /// Resolves the preset that should apply to a conversation: its own
    /// explicit choice, falling back to the global default, falling back to
    /// `None` (caller then falls back further to raw provider config).
    pub async fn resolve_for_conversation(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<Option<ImagePreset>, MythicError> {
        let mut result = db
            .query("SELECT image_preset_id FROM type::record('conversations', $id)")
            .bind(("id", conversation_id.to_string()))
            .await?;
        let row: Option<ConvPresetRef> = crate::db::value_bridge::from_value_opt(result.take(0)?)?;

        if let Some(preset_thing) = row.and_then(|r| r.image_preset_id) {
            match Self::get(
                db,
                &crate::db::value_bridge::record_id_to_string(&preset_thing),
            )
            .await
            {
                Ok(preset) => return Ok(Some(preset)),
                // The preset was deleted out from under this conversation
                // (the cascade event should normally clear image_preset_id
                // too, but tolerate the race) — fall through to the global
                // default. Any other error is real and should propagate.
                Err(MythicError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }

        Self::get_default(db).await
    }
}
