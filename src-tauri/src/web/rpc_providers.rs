//! Providers and API keys, image presets, lorebook, memories, and voice (TTS)
//! commands that are safe to call from a browser.
//!
//! Deliberately not listed (host-global, or would act on the host machine):
//! `tts_model_status`, `tts_download_model`, `tts_preload_engine` (the shared
//! Kokoro model files on the host), and `tts_replay_message` (streams audio
//! through app-wide events that currently reach the desktop window, not the
//! requesting browser; it comes with the event bridge).

use crate::commands::image_presets as ip;
use crate::commands::image_presets::{CreateImagePresetFields, UpdateImagePresetFields};
use crate::commands::lorebook as lb;
use crate::commands::memories as mem;
use crate::commands::providers as pr;
use crate::commands::tts;
use crate::models::DynamicJson;

crate::rpc_family! {
    "create_provider" => pr::create_provider [state] (name: String, provider_type: String, adapter: String, config: DynamicJson, is_default: Option<bool>);
    "get_provider" => pr::get_provider [state] (id: String);
    "list_providers" => pr::list_providers [state] (provider_type: Option<String>);
    "update_provider" => pr::update_provider [state] (id: String, name: Option<String>, config: Option<DynamicJson>);
    "delete_provider" => pr::delete_provider [state] (id: String);
    "set_default_provider" => pr::set_default_provider [state] (id: String);
    "test_provider_connection" => pr::test_provider_connection [state] (id: String);
    "list_wangp_models" => pr::list_wangp_models [state] (id: String);
    "list_provider_models" => pr::list_provider_models [state] (id: String);
    "list_all_models" => pr::list_all_models [state] ();
    "list_embedding_models" => pr::list_embedding_models [state] ();
    "toggle_model_enabled" => pr::toggle_model_enabled [state] (provider_id: String, model_id: String, model_type: String, enabled: bool);
    "list_enabled_models" => pr::list_enabled_models [state] (provider_id: Option<String>);

    "list_image_presets" => ip::list_image_presets [state] ();
    "create_image_preset" => ip::create_image_preset [state] (name: String, fields: CreateImagePresetFields);
    "update_image_preset" => ip::update_image_preset [state] (id: String, fields: UpdateImagePresetFields);
    "delete_image_preset" => ip::delete_image_preset [state] (id: String);
    "set_default_image_preset" => ip::set_default_image_preset [state] (id: String);

    "list_lorebook_entries" => lb::list_lorebook_entries [state] (character_id: String);
    "create_lorebook_entry" => lb::create_lorebook_entry [state] (character_id: Option<String>, name: String, keys: Vec<String>, content: String, always_active: bool);
    "update_lorebook_entry" => lb::update_lorebook_entry [state] (id: String, name: String, keys: Vec<String>, content: String, always_active: bool, priority: i32, insertion_order: i32);
    "toggle_lorebook_entry" => lb::toggle_lorebook_entry [state] (id: String, enabled: bool);
    "delete_lorebook_entry" => lb::delete_lorebook_entry [state] (id: String);
    "import_character_book_entries" => lb::import_character_book_entries [state] (character_id: String);
    "generate_character_lorebook" => lb::generate_character_lorebook [state] (character_id: String, conversation_id: String);

    "list_memories" => mem::list_memories [state] (character_id: Option<String>, conversation_id: Option<String>);
    "create_memory" => mem::create_memory [state] (character_id: Option<String>, conversation_id: Option<String>, content: String, source: Option<String>);
    "update_memory" => mem::update_memory [state] (memory_id: String, content: String);
    "set_memory_importance" => mem::set_memory_importance [state] (memory_id: String, importance: i32);
    "delete_memory" => mem::delete_memory [state] (memory_id: String);
    "promote_to_canon" => mem::promote_to_canon [state] (memory_id: String);
    "share_memory" => mem::share_memory [state] (source_memory_id: String, target_conversation_id: String, link_type: Option<String>, direction: Option<String>, sync_mode: Option<String>);
    "unlink_memory" => mem::unlink_memory [state] (link_id: String);
    "get_memory_graph" => mem::get_memory_graph [state] (character_id: String);

    "tts_set_character_voice" => tts::tts_set_character_voice [state] (character_id: String, voice_id: Option<String>, voice_provider_id: Option<String>);
}

crate::rpc_family_wry! {
    "tts_list_voices" => tts::tts_list_voices [app, state] (provider_id: Option<String>);
    "tts_test_speak" => tts::tts_test_speak [app, state] (text: String, voice_id: String, provider_id: Option<String>);
}
