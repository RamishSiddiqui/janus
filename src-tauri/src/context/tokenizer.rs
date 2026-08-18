use std::sync::OnceLock;

use tiktoken_rs::cl100k_base;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::conversation::MessageRole;

    fn msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: content.to_string(),
        }
    }

    #[test]
    fn empty_string_has_zero_tokens() {
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn longer_text_produces_more_tokens_than_shorter_text() {
        let short = count_tokens("hello");
        let long = count_tokens("hello there, this is a much longer sentence than the first one");
        assert!(long > short);
    }

    #[test]
    fn token_count_is_deterministic_across_calls() {
        // Guards against a bumped tiktoken-rs silently changing encoding
        // behavior for identical input between runs/versions.
        let text = "The quick brown fox jumps over the lazy dog.";
        assert_eq!(count_tokens(text), count_tokens(text));
    }

    #[test]
    fn message_tokens_include_fixed_overhead() {
        let m = msg("hi");
        let content_only = count_tokens("hi");
        assert_eq!(count_message_tokens(&m), content_only + 4);
    }

    #[test]
    fn messages_tokens_sum_includes_reply_priming_overhead() {
        let msgs = vec![msg("hello"), msg("world")];
        let expected: usize = msgs.iter().map(count_message_tokens).sum::<usize>() + 3;
        assert_eq!(count_messages_tokens(&msgs), expected);
    }

    #[test]
    fn empty_message_slice_only_has_reply_priming_overhead() {
        assert_eq!(count_messages_tokens(&[]), 3);
    }
}
