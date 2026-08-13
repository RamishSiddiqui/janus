use surrealdb::Surreal;
use surrealdb::engine::local::Db;


use crate::error::MythicError;
use crate::models::conversation_character::ConversationCharacter;

pub struct ConversationCharacterRepo;

impl ConversationCharacterRepo {
    /// Lists all characters in a conversation (active and inactive), ordered by role priority.
    pub async fn list(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<Vec<ConversationCharacter>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM conversation_characters \
                 WHERE conversation_id = type::thing('conversations', $conv_id) \
                 ORDER BY role ASC, created_at ASC"
            )
            .bind(("conv_id", conversation_id.to_string()))
            .await?;
        let chars: Vec<ConversationCharacter> = result.take(0)?;
        Ok(chars)
    }

    /// Adds a character to a conversation.
    pub async fn add(
        db: &Surreal<Db>,
        conversation_id: &str,
        character_id: &str,
        character_name: &str,
        role: &str,
        talkativeness: i32,
    ) -> Result<ConversationCharacter, MythicError> {
        let composite_id = format!("cc_{}_{}", conversation_id, character_id);
        let talkativeness = talkativeness.clamp(0, 100);

        let mut result = db
            .query(
                "UPSERT type::thing('conversation_characters', $id) CONTENT { \
                    conversation_id: type::thing('conversations', $conv_id), \
                    character_id: type::thing('characters', $char_id), \
                    role: $role, \
                    talkativeness: $talkativeness, \
                    is_active: true, \
                    character_name: $char_name, \
                    created_at: time::now(), \
                }"
            )
            .bind(("id", composite_id))
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("char_id", character_id.to_string()))
            .bind(("role", role.to_string()))
            .bind(("talkativeness", talkativeness))
            .bind(("char_name", character_name.to_string()))
            .await?;

        let cc: Option<ConversationCharacter> = result.take(0)?;
        cc.ok_or_else(|| MythicError::DatabaseOp("Failed to add conversation character".into()))
    }

    /// Removes a character from a conversation.
    pub async fn remove(
        db: &Surreal<Db>,
        conversation_id: &str,
        character_id: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "DELETE FROM conversation_characters WHERE \
             conversation_id = type::thing('conversations', $conv_id) AND \
             character_id = type::thing('characters', $char_id)"
        )
        .bind(("conv_id", conversation_id.to_string()))
        .bind(("char_id", character_id.to_string()))
        .await?;
        Ok(())
    }

    /// Updates the talkativeness of a character in a conversation.
    pub async fn update_talkativeness(
        db: &Surreal<Db>,
        conversation_id: &str,
        character_id: &str,
        talkativeness: i32,
    ) -> Result<(), MythicError> {
        let talkativeness = talkativeness.clamp(0, 100);
        db.query(
            "UPDATE conversation_characters SET talkativeness = $talk WHERE \
             conversation_id = type::thing('conversations', $conv_id) AND \
             character_id = type::thing('characters', $char_id)"
        )
        .bind(("conv_id", conversation_id.to_string()))
        .bind(("char_id", character_id.to_string()))
        .bind(("talk", talkativeness))
        .await?;
        Ok(())
    }

    /// Sets a character's role within a conversation — used to promote a
    /// `'transient'` placeholder (registered the instant it first spoke) to
    /// `'npc'` once a real profile has been generated for it.
    pub async fn set_role(
        db: &Surreal<Db>,
        conversation_id: &str,
        character_id: &str,
        role: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "UPDATE conversation_characters SET role = $role WHERE \
             conversation_id = type::thing('conversations', $conv_id) AND \
             character_id = type::thing('characters', $char_id)"
        )
        .bind(("conv_id", conversation_id.to_string()))
        .bind(("char_id", character_id.to_string()))
        .bind(("role", role.to_string()))
        .await?;
        Ok(())
    }

    /// Toggles whether a character is active (unmuted) in a conversation.
    pub async fn set_active(
        db: &Surreal<Db>,
        conversation_id: &str,
        character_id: &str,
        is_active: bool,
    ) -> Result<(), MythicError> {
        db.query(
            "UPDATE conversation_characters SET is_active = $active WHERE \
             conversation_id = type::thing('conversations', $conv_id) AND \
             character_id = type::thing('characters', $char_id)"
        )
        .bind(("conv_id", conversation_id.to_string()))
        .bind(("char_id", character_id.to_string()))
        .bind(("active", is_active))
        .await?;
        Ok(())
    }
}
