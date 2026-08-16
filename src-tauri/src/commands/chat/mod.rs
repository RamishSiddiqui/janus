//! Chat command handlers — sending, retrying, and regenerating messages;
//! streaming responses from LLM providers; and the background pipelines
//! (embedding, scene extraction, NPC detection) that run after a turn
//! completes. Split by concern — see each submodule's own doc comment.

pub(crate) mod attachments;
pub(crate) mod pipeline;
pub(crate) mod retry;
pub(crate) mod send;
pub(crate) mod streaming;

pub use attachments::{upload_message_attachment, upload_message_attachment_bytes};
pub use pipeline::{extract_initial_scene, generate_raw, get_context_stats};
pub use retry::{cancel_generation, regenerate_message, retry_failed_message};
pub use send::send_message;

/// The IDs of the user/assistant message pair created by `send_message`,
/// `retry_failed_message`, or `regenerate_message` — the frontend uses
/// these to attach the streamed response to the right message bubbles.
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
pub struct SendMessageResult {
    pub user_message_id: String,
    pub assistant_message_id: String,
}
