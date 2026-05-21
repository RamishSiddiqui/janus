use tiktoken_rs::cl100k_base;
use std::sync::OnceLock;

use crate::models::conversation::ChatMessage;

/// Global tokenizer instance — cl100k_base is a reasonable approximation
/// across providers (GPT-4, Claude, Gemini all tokenize similarly enough
/// for budget purposes). The exact count doesn't need to be perfect;
/// we leave a safety margin in the budget calculator.
static BPE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();

fn bpe() -> &'static tiktoken_rs::CoreBPE {
    BPE.get_or_init(|| cl100k_base().expect("Failed to load cl100k_base tokenizer"))
}

/// Count tokens in a single string.
pub fn count_tokens(text: &str) -> usize {
    bpe().encode_ordinary(text).len()
}

/// Count tokens for a single ChatMessage.
/// Includes the overhead of role markers (~4 tokens per message for
/// the <|im_start|>role\n...content...<|im_end|> framing).
pub fn count_message_tokens(message: &ChatMessage) -> usize {
    // Per OpenAI: every message has ~4 tokens of overhead
    // (role name, delimiters). This is a reasonable cross-provider estimate.
    const MESSAGE_OVERHEAD: usize = 4;
    count_tokens(&message.content) + MESSAGE_OVERHEAD
}

/// Count tokens for a slice of messages.
pub fn count_messages_tokens(messages: &[ChatMessage]) -> usize {
    // Base overhead: 3 tokens for the <|im_start|>assistant priming
    const REPLY_PRIMING: usize = 3;
    messages.iter().map(count_message_tokens).sum::<usize>() + REPLY_PRIMING
}
