/// A streaming chunk emitted during text generation.
///
/// Sent via a channel so the frontend can display tokens in real-time.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A text delta (partial token)
    Delta(String),

    /// A chain-of-thought/reasoning delta from a reasoning model (Nemotron,
    /// DeepSeek R1, o1/o3, etc.) — the model's internal "thinking" narration,
    /// kept distinct from `Delta` so the frontend can render it as a
    /// collapsible "thinking" trace instead of as if the character said it.
    ReasoningDelta(String),

    /// Generation is complete — includes the full assembled response
    Done(String),

    /// An error occurred during generation
    Error(String),
}
