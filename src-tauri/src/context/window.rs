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
