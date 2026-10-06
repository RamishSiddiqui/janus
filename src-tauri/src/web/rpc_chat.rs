//! Chat: sending, retrying, regenerating, cancelling, and the one-shot
//! helpers. Streaming events these commands emit reach the browser through a
//! separate event channel; this table only exposes the calls.
//!
//! Not listed on purpose: `upload_message_attachment` and
//! `upload_message_attachment_bytes` (the first reads a path on the host; the
//! browser gets its own upload endpoint).

use crate::commands::chat;
use crate::models::conversation::MessageAttachment;

crate::rpc_family! {
    "generate_raw" => chat::generate_raw [state] (system_prompt: String, user_prompt: String, model: Option<String>, max_tokens: Option<u32>, temperature: Option<f32>);
    "get_context_stats" => chat::get_context_stats [state] (conversation_id: String, message_id: String, system_prompt: Option<String>, post_history_instructions: Option<String>);
}

crate::rpc_family_wry! {
    "send_message" => chat::send_message [app, state] (conversation_id: String, content: String, model: Option<String>, system_prompt: Option<String>, streaming: Option<bool>, post_history_instructions: Option<String>, attachments: Option<Vec<MessageAttachment>>);
    "retry_failed_message" => chat::retry_failed_message [app, state] (conversation_id: String, user_message_id: String, model: Option<String>, system_prompt: Option<String>, streaming: Option<bool>, post_history_instructions: Option<String>);
    "regenerate_message" => chat::regenerate_message [app, state] (conversation_id: String, message_id: String, model: Option<String>, system_prompt: Option<String>, streaming: Option<bool>, post_history_instructions: Option<String>);
    "cancel_generation" => chat::cancel_generation [app, state] (conversation_id: String);
    "extract_initial_scene" => chat::extract_initial_scene [app, state] (conversation_id: String, text: String);
}
