use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::error::MythicError;
use crate::models::summary::ConversationSummary;

pub struct SummaryRepo;

impl SummaryRepo {
    /// Retrieves the rolling summary for a conversation, if one exists.
    /// Each conversation has at most one summary (enforced by unique index).
    pub async fn get(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<Option<ConversationSummary>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM conversation_summaries \
                 WHERE conversation_id = type::record('conversations', $conv_id) \
                 LIMIT 1",
            )
            .bind(("conv_id", conversation_id.to_string()))
            .await?;

        let summaries: Vec<ConversationSummary> =
            crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(summaries.into_iter().next())
    }

    /// Creates or updates the rolling summary for a conversation.
    /// Uses UPSERT semantics — if a summary already exists for the conversation,
    /// it gets updated in place rather than creating a duplicate.
    pub async fn upsert(
        db: &Surreal<Db>,
        conversation_id: &str,
        summary_text: &str,
        covered_message_count: u32,
        token_count: u32,
        window_start_message_id: Option<&str>,
    ) -> Result<(), MythicError> {
        let window_start_thing =
            window_start_message_id.map(|id| surrealdb::types::RecordId::new("messages", id));

        db.query(
            "UPSERT conversation_summaries SET \
                conversation_id = type::record('conversations', $conv_id), \
                summary_text = $text, \
                covered_message_count = $count, \
                token_count = $tokens, \
                window_start_message_id = $window_start, \
                updated_at = time::now() \
             WHERE conversation_id = type::record('conversations', $conv_id)",
        )
        .bind(("conv_id", conversation_id.to_string()))
        .bind(("text", summary_text.to_string()))
        .bind(("count", covered_message_count as i64))
        .bind(("tokens", token_count as i64))
        .bind(("window_start", window_start_thing))
        .await?;

        Ok(())
    }

    /// Deletes the summary for a conversation.
    /// Called when a conversation is reset or deleted.
    pub async fn delete(db: &Surreal<Db>, conversation_id: &str) -> Result<(), MythicError> {
        db.query(
            "DELETE FROM conversation_summaries \
             WHERE conversation_id = type::record('conversations', $conv_id)",
        )
        .bind(("conv_id", conversation_id.to_string()))
        .await?;

        Ok(())
    }
}
