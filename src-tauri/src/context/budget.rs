use crate::context::tokenizer::count_messages_tokens;
use crate::models::conversation::ChatMessage;

/// Configuration for the context window budget.
/// All values are in tokens.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Total context window size in tokens (e.g., 8192, 32768, 131072).
    /// This should match the model's context length.
    pub max_context_tokens: usize,
    /// Tokens reserved for the model's response (max_tokens generation param).
    pub reserved_for_response: usize,
    /// Safety margin as a fraction (0.0–1.0). We use 90% of available space
    /// to account for tokenizer approximation differences across providers.
    pub safety_margin: f64,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_context_tokens: 16384,
            reserved_for_response: 2048,
            safety_margin: 0.90,
        }
    }
}

/// Result of a budget calculation — tells the window manager how many
/// tokens are available for conversation history and summary.
#[derive(Debug, Clone)]
pub struct BudgetAllocation {
    /// Total usable tokens after safety margin and response reservation.
    pub total_usable: usize,
    /// Tokens consumed by fixed layers (system prompt, character, lorebook, memories, emotion, PHI).
    pub fixed_layers_tokens: usize,
    /// Tokens available for conversation history (the sliding window).
    pub history_budget: usize,
    /// Tokens available for the rolling summary (subset of history_budget).
    /// Summaries get up to 20% of history budget; rest goes to verbatim messages.
    pub summary_budget: usize,
    /// Tokens available for verbatim recent messages.
    pub messages_budget: usize,
}

impl ContextBudget {
    /// Calculate how many tokens are available for conversation history,
    /// given the fixed prompt layers that have already been assembled.
    ///
    /// `fixed_layers` — all system messages EXCEPT conversation history
    /// (system prompt, character card, lorebook, memories, emotional state, PHI).
    pub fn allocate(&self, fixed_layers: &[ChatMessage]) -> BudgetAllocation {
        let total_usable = ((self.max_context_tokens - self.reserved_for_response) as f64
            * self.safety_margin) as usize;

        let fixed_layers_tokens = count_messages_tokens(fixed_layers);

        let history_budget = total_usable.saturating_sub(fixed_layers_tokens);

        // Summary gets up to 20% of the history budget.
        // This keeps summaries concise while leaving room for verbatim messages.
        let summary_budget = history_budget / 5;
        let messages_budget = history_budget.saturating_sub(summary_budget);

        BudgetAllocation {
            total_usable,
            fixed_layers_tokens,
            history_budget,
            summary_budget,
            messages_budget,
        }
    }
}
