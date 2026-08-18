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
    /// Hard cap on tokens for the memories layer.
    pub max_memory_tokens: usize,
    /// Hard cap on tokens for the lorebook layer.
    pub max_lorebook_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_context_tokens: 16384,
            reserved_for_response: 2048,
            safety_margin: 0.90,
            max_memory_tokens: 800,
            max_lorebook_tokens: 1500,
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
    /// Summaries get up to 20% of history budget.
    pub summary_budget: usize,
    /// Tokens available for RAG retrieval (subset of history_budget).
    /// RAG gets up to 10% of history budget.
    pub rag_budget: usize,
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
        // RAG gets up to 10% of the history budget.
        let rag_budget = history_budget / 10;
        let messages_budget = history_budget
            .saturating_sub(summary_budget)
            .saturating_sub(rag_budget);

        BudgetAllocation {
            total_usable,
            fixed_layers_tokens,
            history_budget,
            summary_budget,
            rag_budget,
            messages_budget,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::conversation::MessageRole;

    fn msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::System,
            content: content.to_string(),
        }
    }

    #[test]
    fn default_budget_matches_documented_constants() {
        let budget = ContextBudget::default();
        assert_eq!(budget.max_context_tokens, 16384);
        assert_eq!(budget.reserved_for_response, 2048);
        assert_eq!(budget.max_memory_tokens, 800);
        assert_eq!(budget.max_lorebook_tokens, 1500);
    }

    #[test]
    fn total_usable_applies_response_reservation_and_safety_margin() {
        let budget = ContextBudget::default();
        let allocation = budget.allocate(&[]);
        // (16384 - 2048) * 0.90 = 12902.4 -> truncated to 12902
        assert_eq!(allocation.total_usable, 12902);
    }

    #[test]
    fn history_budget_shrinks_as_fixed_layers_grow() {
        let budget = ContextBudget::default();
        let empty_allocation = budget.allocate(&[]);

        let fixed_layers = vec![msg(&"system prompt content ".repeat(20))];
        let with_fixed_layers = budget.allocate(&fixed_layers);

        assert!(with_fixed_layers.fixed_layers_tokens > 0);
        assert!(with_fixed_layers.history_budget < empty_allocation.history_budget);
    }

    #[test]
    fn fixed_layers_exceeding_total_usable_saturate_history_budget_to_zero() {
        let budget = ContextBudget {
            max_context_tokens: 100,
            reserved_for_response: 10,
            safety_margin: 1.0,
            ..ContextBudget::default()
        };
        // A fixed-layers block far larger than the tiny total_usable above.
        let fixed_layers = vec![msg(&"word ".repeat(500))];
        let allocation = budget.allocate(&fixed_layers);

        assert_eq!(allocation.history_budget, 0);
        assert_eq!(allocation.summary_budget, 0);
        assert_eq!(allocation.rag_budget, 0);
        assert_eq!(allocation.messages_budget, 0);
    }

    #[test]
    fn summary_and_rag_budgets_are_fixed_fractions_of_history_budget() {
        let budget = ContextBudget::default();
        let allocation = budget.allocate(&[]);

        assert_eq!(allocation.summary_budget, allocation.history_budget / 5);
        assert_eq!(allocation.rag_budget, allocation.history_budget / 10);
        assert_eq!(
            allocation.messages_budget,
            allocation.history_budget - allocation.summary_budget - allocation.rag_budget
        );
    }
}
