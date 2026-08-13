use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::MythicError;
use crate::models::ai_horde_model::AiHordeModelInfo;

pub struct AiHordeModelRepo;

/// Turns a model name (e.g. "AlbedoBase XL (SDXL)") into a safe SurrealDB
/// record-id fragment.
fn sanitize_id(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

impl AiHordeModelRepo {
    /// Upserts capability info for a batch of models in one go — called
    /// whenever the AI Horde model list is fetched, so the cache stays
    /// reasonably fresh without a dedicated refresh job.
    pub async fn upsert_many(db: &Surreal<Db>, models: &[AiHordeModelInfo]) -> Result<(), MythicError> {
        for m in models {
            db.query(
                "UPSERT type::thing('ai_horde_model_info', $id) MERGE {
                    name: $name,
                    baseline: $baseline,
                    inpainting: $inpainting,
                    nsfw: $nsfw,
                    style: $style,
                    img2img_supported: $img2img_supported,
                    worker_count: $worker_count,
                    updated_at: time::now(),
                }",
            )
            .bind(("id", sanitize_id(&m.name)))
            .bind(("name", m.name.clone()))
            .bind(("baseline", m.baseline.clone()))
            .bind(("inpainting", m.inpainting))
            .bind(("nsfw", m.nsfw))
            .bind(("style", m.style.clone()))
            .bind(("img2img_supported", m.img2img_supported))
            .bind(("worker_count", m.worker_count))
            .await?
            .check()
            .map_err(|e| MythicError::DatabaseOp(format!("ai_horde_model upsert: {}", e)))?;
        }
        Ok(())
    }

    pub async fn get(db: &Surreal<Db>, name: &str) -> Result<Option<AiHordeModelInfo>, MythicError> {
        let row: Option<AiHordeModelInfo> = db.select(("ai_horde_model_info", sanitize_id(name))).await?;
        Ok(row)
    }

    pub async fn list(db: &Surreal<Db>) -> Result<Vec<AiHordeModelInfo>, MythicError> {
        let mut result = db.query("SELECT * FROM ai_horde_model_info ORDER BY name ASC").await?;
        let rows: Vec<AiHordeModelInfo> = result.take(0)?;
        Ok(rows)
    }
}
