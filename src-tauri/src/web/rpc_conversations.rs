//! Conversations and everything under them: messages, cast, emotion and scene
//! state, trash, embeddings, scenes, and the NPC pipeline.
//!
//! Not exposed: `get_scene_path` (returns an absolute path on the host; the
//! browser gets an owner-checked file endpoint instead).

use crate::commands::character_state as cs;
use crate::commands::conversation_characters as cc;
use crate::commands::conversations as cv;
use crate::commands::embeddings as em;
use crate::commands::messages as ms;
use crate::commands::npc;
use crate::commands::scene_states as ss;
use crate::commands::scenes::{self as sc, GenerateSceneOptions, GenerateVideoOptions};
use crate::commands::trash;
use crate::models::DynamicJson;

// Commands that only need `State`.
crate::rpc_family! {
    "create_conversation" => cv::create_conversation [state] (character_id: Option<String>, title: Option<String>, persona_id: Option<String>);
    "get_conversation" => cv::get_conversation [state] (id: String);
    "list_conversations" => cv::list_conversations [state] (limit: Option<u32>, offset: Option<u32>);
    "count_conversations" => cv::count_conversations [state] ();
    "trash_conversation" => cv::trash_conversation [state] (id: String);
    "restore_conversation" => cv::restore_conversation [state] (id: String);
    "get_conversation_messages" => cv::get_conversation_messages [state] (conversation_id: String);
    "set_active_message" => cv::set_active_message [state] (conversation_id: String, message_id: String);
    "set_conversation_image_preset" => cv::set_conversation_image_preset [state] (conversation_id: String, preset_id: Option<String>);
    "set_conversation_persona" => cv::set_conversation_persona [state] (conversation_id: String, persona_id: Option<String>);
    "update_conversation" => cv::update_conversation [state] (id: String, title: String);
    "set_memory_scope" => cv::set_memory_scope [state] (conversation_id: String, scope: String);
    "branch_conversation" => cv::branch_conversation [state] (parent_conversation_id: String, branch_point_message_id: String, new_title: Option<String>);
    "search_messages" => cv::search_messages [state] (query: String, limit: Option<u32>);

    "list_conversation_characters" => cc::list_conversation_characters [state] (conversation_id: String);
    "add_conversation_character" => cc::add_conversation_character [state] (conversation_id: String, character_id: String, character_name: String, role: Option<String>, talkativeness: Option<i32>);
    "remove_conversation_character" => cc::remove_conversation_character [state] (conversation_id: String, character_id: String);
    "update_character_talkativeness" => cc::update_character_talkativeness [state] (conversation_id: String, character_id: String, talkativeness: i32);
    "toggle_character_active" => cc::toggle_character_active [state] (conversation_id: String, character_id: String, is_active: bool);

    "create_message" => ms::create_message [state] (conversation_id: String, role: String, content: String, parent_id: Option<String>, metadata: Option<DynamicJson>);
    "update_message" => ms::update_message [state] (id: String, content: String);
    "delete_message" => ms::delete_message [state] (id: String);
    "get_message_branch" => ms::get_message_branch [state] (message_id: String);
    "get_message_siblings" => ms::get_message_siblings [state] (message_id: String);

    "get_character_state" => cs::get_character_state [state] (character_id: String, conversation_id: String);
    "upsert_character_state" => cs::upsert_character_state [state] (character_id: String, conversation_id: String, mood: i32, trust: i32, arousal: i32, dominant_emotion: String, state_summary: String);
    "set_message_emotional_snapshot" => cs::set_message_emotional_snapshot [state] (message_id: String, states: DynamicJson);

    "get_scene_state" => ss::get_scene_state [state] (conversation_id: String);
    "upsert_scene_state" => ss::upsert_scene_state [state] (conversation_id: String, location_name: Option<String>, location_description: Option<String>, time_period: Option<String>, weather: Option<String>, characters_present: Option<Vec<String>>, ambient_details: Option<String>, scene_mood: Option<String>);
    "delete_scene_state" => ss::delete_scene_state [state] (conversation_id: String);

    "list_trash" => trash::list_trash [state] ();
    "empty_trash" => trash::empty_trash [state] ();

    "get_embedding_index_status" => em::get_embedding_index_status [state] (conversation_id: Option<String>, selected_model: Option<String>);

    "list_scene_cast_members" => sc::list_scene_cast_members [state] (conversation_id: String);
    "cancel_scene_generation" => sc::cancel_scene_generation [state] (conversation_id: String);
    "list_scenes" => sc::list_scenes [state] (conversation_id: String);

    "list_conversation_npcs" => npc::list_conversation_npcs [state] (conversation_id: String);
    "promote_npc_to_gallery" => npc::promote_npc_to_gallery [state] (character_id: String);
    "confirm_npc" => npc::confirm_npc [state] (conversation_id: String, character_id: String);
    "mark_npc_reviewed" => npc::mark_npc_reviewed [state] (character_id: String);
    "refresh_character_profile" => npc::refresh_character_profile [state] (character_id: String, conversation_id: String, system_prompt: Option<String>);
    "approve_npc_portrait" => npc::approve_npc_portrait [state] (character_id: String);
    "reject_npc_portrait" => npc::reject_npc_portrait [state] (character_id: String);
    "get_cast_memory_graph" => npc::get_cast_memory_graph [state] (conversation_id: String);
}

// Commands whose signature includes the real window's `AppHandle`.
crate::rpc_family_wry! {
    "generate_persona_portrait" => crate::commands::personas::generate_persona_portrait [app, state] (persona_id: String, conversation_id: Option<String>);
    "delete_conversation" => cv::delete_conversation [app, state] (id: String);

    "rebuild_embedding_index" => em::rebuild_embedding_index [state, app] (conversation_id: Option<String>, embedding_model: String);
    "backfill_missing_embeddings" => em::backfill_missing_embeddings [state, app] (conversation_id: Option<String>);

    "generate_scene" => sc::generate_scene [app, state] (conversation_id: String, message_id: Option<String>, prompt: String, options: GenerateSceneOptions);
    "generate_video_scene" => sc::generate_video_scene [app, state] (conversation_id: String, message_id: Option<String>, prompt: String, options: GenerateVideoOptions);
    "delete_scene" => sc::delete_scene [app, state] (scene_id: String);

    "generate_npc_portrait" => npc::generate_npc_portrait [app, state] (character_id: String, conversation_id: String, auto_approve: bool);
    "debug_run_npc_detection" => npc::debug_run_npc_detection [app, state] (conversation_id: String, ai_response: String);
}
