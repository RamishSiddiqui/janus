use crate::context::tokenizer::count_message_tokens;
use crate::models::conversation::ChatMessage;

/// Result of applying the sliding window to a conversation history.
#[derive(Debug)]
pub struct WindowResult {
    /// Messages that fit within the token budget (in chronological order).
    pub included: Vec<ChatMessage>,
    /// Number of messages that were evicted (didn't fit in the window).
    pub evicted_count: usize,
    /// Total tokens consumed by the included messages.
    pub included_tokens: usize,
}

/// Applies a token-budgeted sliding window to the conversation history.
///
/// Walks backwards from the most recent message, including messages until
/// the token budget is exhausted. Returns the messages in chronological
/// order (oldest included first) to maintain conversation flow.
///
/// # Arguments
/// * `chain` — Full conversation history in chronological order (root → leaf).
/// * `token_budget` — Maximum tokens available for conversation messages.
pub fn apply_sliding_window(chain: &[ChatMessage], token_budget: usize) -> WindowResult {
    if chain.is_empty() || token_budget == 0 {
        return WindowResult {
            included: Vec::new(),
            evicted_count: chain.len(),
            included_tokens: 0,
        };
    }

    let mut included_tokens: usize = 0;
    let mut include_from: usize = chain.len(); // index where we start including

    // Walk backwards from the most recent message
    for (i, msg) in chain.iter().enumerate().rev() {
        let msg_tokens = count_message_tokens(msg);

        if included_tokens + msg_tokens > token_budget {
            // This message doesn't fit — stop here.
            // But always include at least the last message (the user's current input).
            if include_from == chain.len() {
                // Haven't included anything yet — force-include the last message
                include_from = chain.len() - 1;
                included_tokens = msg_tokens;
            }
            break;
        }

        included_tokens += msg_tokens;
        include_from = i;
    }

    let evicted_count = include_from;
    let included = chain[include_from..].to_vec();

    WindowResult {
        included,
        evicted_count,
        included_tokens,
    }
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
    fn empty_chain_returns_empty_result() {
        let result = apply_sliding_window(&[], 1000);
        assert!(result.included.is_empty());
        assert_eq!(result.evicted_count, 0);
        assert_eq!(result.included_tokens, 0);
    }

    #[test]
    fn zero_budget_evicts_everything() {
        let chain = vec![msg("hello"), msg("world")];
        let result = apply_sliding_window(&chain, 0);
        assert!(result.included.is_empty());
        assert_eq!(result.evicted_count, 2);
    }

    #[test]
    fn everything_fits_when_budget_is_generous() {
        let chain = vec![msg("hello"), msg("world"), msg("how are you")];
        let result = apply_sliding_window(&chain, 10_000);
        assert_eq!(result.included.len(), 3);
        assert_eq!(result.evicted_count, 0);
        // Chronological order is preserved.
        assert_eq!(result.included[0].content, "hello");
        assert_eq!(result.included[2].content, "how are you");
    }

    #[test]
    fn oldest_messages_are_evicted_first_when_budget_is_tight() {
        // Each short message costs roughly the same number of tokens
        // (content + fixed per-message overhead), so a budget sized for
        // ~2 messages should keep only the most recent ones.
        let chain = vec![msg("one"), msg("two"), msg("three"), msg("four")];
        let per_message = crate::context::tokenizer::count_message_tokens(&msg("four"));
        let result = apply_sliding_window(&chain, per_message * 2);

        assert!(result.included.len() < chain.len());
        assert_eq!(result.evicted_count, chain.len() - result.included.len());
        // Most recent message must always survive.
        assert_eq!(
            result.included.last().unwrap().content,
            chain.last().unwrap().content
        );
    }

    #[test]
    fn always_force_includes_the_last_message_even_if_it_alone_exceeds_budget() {
        let chain = vec![msg("short"), msg(&"a very long message ".repeat(50))];
        // Budget too small for even the single last message.
        let result = apply_sliding_window(&chain, 1);
        assert_eq!(result.included.len(), 1);
        assert_eq!(result.included[0].content, chain[1].content);
        assert_eq!(result.evicted_count, 1);
    }
}
