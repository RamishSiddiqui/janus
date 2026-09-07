//! Sentence-boundary splitter for streaming TTS synthesis — turns
//! incrementally-arriving LLM text deltas into complete sentences as soon
//! as they're available, so synthesis can start well before the full
//! message finishes generating (mirrors the pattern used across every real
//! streaming-TTS implementation researched for issue #66: sentence-level
//! chunking with a minimum-length guard, not full NLP-grade segmentation).

/// Minimum sentence length (chars) before a `.`/`!`/`?` boundary is treated
/// as a real end-of-sentence rather than (most commonly) a decimal point or
/// a title abbreviation ("Mr.", "Dr.") — a cheap heuristic, not full
/// sentence detection, so it doesn't fire a TTS chunk on a two-character
/// fragment.
const MIN_SENTENCE_LEN: usize = 8;

/// Splits `buffer` into complete sentences plus whatever trailing partial
/// text remains. The caller is expected to keep accumulating deltas into
/// the returned remainder and call this again as more text arrives.
pub fn split_complete_sentences(buffer: &str) -> (Vec<String>, String) {
    let mut sentences = Vec::new();
    let chars: Vec<(usize, char)> = buffer.char_indices().collect();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        let (byte_idx, ch) = chars[i];
        if matches!(ch, '.' | '!' | '?') {
            // Absorb a run of repeated terminators ("...", "?!") into one boundary.
            let mut end_idx = byte_idx + ch.len_utf8();
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j].1, '.' | '!' | '?') {
                end_idx = chars[j].0 + chars[j].1.len_utf8();
                j += 1;
            }
            let followed_by_boundary = j >= chars.len() || chars[j].1.is_whitespace();
            let candidate = &buffer[start..end_idx];
            if followed_by_boundary && candidate.trim().len() >= MIN_SENTENCE_LEN {
                sentences.push(candidate.trim().to_string());
                let mut k = j;
                while k < chars.len() && chars[k].1.is_whitespace() {
                    k += 1;
                }
                start = if k < chars.len() {
                    chars[k].0
                } else {
                    buffer.len()
                };
                i = k;
                continue;
            }
            i = j;
            continue;
        }
        i += 1;
    }

    let remainder = buffer[start..].to_string();
    (sentences, remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_sentence_boundaries() {
        let (sentences, remainder) =
            split_complete_sentences("Hello there, friend. How are you today? I am ");
        assert_eq!(
            sentences,
            vec![
                "Hello there, friend.".to_string(),
                "How are you today?".to_string()
            ]
        );
        assert_eq!(remainder, "I am ");
    }

    #[test]
    fn does_not_split_short_fragments() {
        let (sentences, remainder) = split_complete_sentences("Ok. ");
        assert!(sentences.is_empty());
        assert_eq!(remainder, "Ok. ");
    }

    #[test]
    fn absorbs_ellipsis_and_multi_punct() {
        let (sentences, remainder) =
            split_complete_sentences("Wait a moment... is that really true?! Yes");
        assert_eq!(
            sentences,
            vec![
                "Wait a moment...".to_string(),
                "is that really true?!".to_string()
            ]
        );
        assert_eq!(remainder, "Yes");
    }
}
