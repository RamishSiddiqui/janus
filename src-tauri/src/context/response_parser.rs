//! Multi-character response parser.
//!
//! Splits a single LLM response containing multiple character voices into
//! individual segments, each attributed to a specific character.
//!
//! Expected format: `[CharName]: content` separated by double newlines.
//! Falls back gracefully — if no markers are found, the entire response
//! is treated as the primary character's response.

use regex::Regex;
use tracing::debug;

/// A single character's segment from a multi-character response.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ParsedSegment {
    /// The character's name as it appeared in the `[Name]:` marker
    pub character_name: String,
    /// The character's response content (without the `[Name]:` prefix)
    pub content: String,
    /// Position in the original response (0-indexed)
    pub index: usize,
}

/// Parses a multi-character response into individual character segments.
///
/// Uses the provided `known_names` to build a targeted regex. If no known
/// character markers are found, falls back to treating the entire response
/// as a single segment attributed to `fallback_name`.
///
/// # Arguments
/// * `response` - The full LLM response text
/// * `known_names` - Character names to look for (e.g., ["Elara", "Kael"])
/// * `fallback_name` - Name to use if no character markers are found
pub fn parse_multi_character_response(
    response: &str,
    known_names: &[String],
    fallback_name: &str,
) -> Vec<ParsedSegment> {
    if response.trim().is_empty() {
        return vec![];
    }

    // Build regex pattern from known character names.
    // Include both full names AND first names, since LLMs often write
    // [Roran]: instead of [Roran Ironfist]:
    let mut name_variants: Vec<String> = Vec::new();
    for name in known_names {
        name_variants.push(regex::escape(name));
        // Also add first name if the full name has multiple parts
        if let Some(first) = name.split_whitespace().next() {
            if first != name {
                name_variants.push(regex::escape(first));
            }
        }
    }
    // Sort by length descending so "Aria Silverleaf" is tried before "Aria"
    name_variants.sort_by(|a, b| b.len().cmp(&a.len()));
    name_variants.dedup();

    // Matches: [CharName]: at the start of a line (possibly after newlines).
    // Always unions in a generic capitalized-name pattern alongside the
    // known-names-specific one — a solo conversation can now legitimately
    // introduce a brand-new speaker's marker (see chat.rs's "Other Characters
    // Present" prompt addition), and that name won't be in `known_names` yet.
    const GENERIC_NAME_PATTERN: &str = r"[A-Z][\w' .-]{1,40}";
    let known_alt = name_variants.join("|");
    let pattern = if known_alt.is_empty() {
        format!(r"(?m)^\[({})\]:\s*", GENERIC_NAME_PATTERN)
    } else {
        format!(r"(?m)^\[({}|{})\]:\s*", known_alt, GENERIC_NAME_PATTERN)
    };

    let re = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(e) => {
            debug!("[response_parser] Failed to compile regex: {}", e);
            return vec![ParsedSegment {
                character_name: fallback_name.to_string(),
                content: response.trim().to_string(),
                index: 0,
            }];
        }
    };

    // Find all matches and their positions
    let matches: Vec<_> = re.find_iter(response).collect();

    if matches.is_empty() {
        // No character markers found — check for a looser format: "CharName:"
        // (without brackets) at line start
        let loose_pattern = format!(r"(?m)^({}):\s*", name_variants.join("|"));
        if let Ok(loose_re) = Regex::new(&loose_pattern) {
            let loose_matches: Vec<_> = loose_re.find_iter(response).collect();
            if !loose_matches.is_empty() {
                return parse_with_regex(response, &loose_re, known_names, fallback_name);
            }
        }

        // Complete fallback — no markers at all
        // Heuristic: check if the response starts with a known character's name
        // (e.g., "Finn holds Aria's gaze..." → attribute to Finn)
        let trimmed = response.trim();
        // Strip leading RP formatting (* for actions, " for speech)
        let stripped = trimmed.trim_start_matches(|c: char| c == '*' || c == '"' || c == '\u{201C}' || c == '_');
        let first_word = stripped.split(|c: char| c.is_whitespace() || c == '\'' || c == '\u{2019}' || c == '*').next().unwrap_or("");
        let first_word_lower = first_word.to_lowercase();

        for name in known_names {
            let name_lower = name.to_lowercase();
            let first_name_lower = name.split_whitespace().next().unwrap_or("").to_lowercase();

            // Match if response starts with either the full name or first name
            if first_word_lower == first_name_lower
                || trimmed.to_lowercase().starts_with(&name_lower)
            {
                debug!(
                    "[response_parser] No markers found but response starts with '{}' — attributing to {}",
                    first_word, name
                );
                return vec![ParsedSegment {
                    character_name: name.clone(),
                    content: trimmed.to_string(),
                    index: 0,
                }];
            }
        }

        debug!("[response_parser] No character markers found, using fallback");
        return vec![ParsedSegment {
            character_name: fallback_name.to_string(),
            content: trimmed.to_string(),
            index: 0,
        }];
    }

    parse_with_regex(response, &re, known_names, fallback_name)
}

/// Internal helper — splits response using the compiled regex.
fn parse_with_regex(
    response: &str,
    re: &Regex,
    _known_names: &[String],
    fallback_name: &str,
) -> Vec<ParsedSegment> {
    let mut segments: Vec<ParsedSegment> = Vec::new();
    let mut last_end: usize = 0;
    let mut last_name: Option<String> = None;

    for mat in re.find_iter(response) {
        // Content before this marker belongs to the previous character
        if let Some(ref prev_name) = last_name {
            let content = response[last_end..mat.start()].trim();
            if !content.is_empty() {
                segments.push(ParsedSegment {
                    character_name: prev_name.clone(),
                    content: content.to_string(),
                    index: segments.len(),
                });
            }
        } else {
            // Content before the first marker (preamble) — attribute to fallback
            let preamble = response[..mat.start()].trim();
            if !preamble.is_empty() {
                segments.push(ParsedSegment {
                    character_name: fallback_name.to_string(),
                    content: preamble.to_string(),
                    index: segments.len(),
                });
            }
        }

        // Extract character name from the marker
        let marker_text = mat.as_str();
        let name = if let Some(caps) = re.captures(marker_text) {
            caps.get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| fallback_name.to_string())
        } else {
            fallback_name.to_string()
        };

        last_name = Some(name);
        last_end = mat.end();
    }

    // Content after the last marker
    if let Some(ref name) = last_name {
        let content = response[last_end..].trim();
        if !content.is_empty() {
            segments.push(ParsedSegment {
                character_name: name.clone(),
                content: content.to_string(),
                index: segments.len(),
            });
        }
    }

    // If parsing produced nothing, fall back to full response
    if segments.is_empty() {
        segments.push(ParsedSegment {
            character_name: fallback_name.to_string(),
            content: response.trim().to_string(),
            index: 0,
        });
    }

    // Merge adjacent segments attributed to the same character. Smaller/free
    // models occasionally re-emit a redundant `[Name]:` marker partway
    // through a long completion even though no other character actually
    // spoke in between — left unmerged, that produces two separate message
    // bubbles for one speaker back-to-back, which reads as a bug in the UI,
    // not a stylistic choice.
    let mut merged: Vec<ParsedSegment> = Vec::with_capacity(segments.len());
    for seg in segments {
        if let Some(last) = merged.last_mut() {
            if last.character_name == seg.character_name {
                last.content.push_str("\n\n");
                last.content.push_str(&seg.content);
                continue;
            }
        }
        merged.push(seg);
    }
    for (i, seg) in merged.iter_mut().enumerate() {
        seg.index = i;
    }
    let segments = merged;

    debug!(
        "[response_parser] Parsed {} segments from {} chars of response",
        segments.len(),
        response.len()
    );

    segments
}

/// Resolves a parsed character name to its ID from the conversation_characters list.
/// Supports exact match, case-insensitive match, first-name match, and
/// substring match to handle LLMs that abbreviate character names.
pub fn resolve_character_id(
    name: &str,
    known_chars: &[(String, String)], // (name, id) pairs
) -> Option<String> {
    // 1. Exact match
    if let Some((_, id)) = known_chars.iter().find(|(n, _)| n == name) {
        return Some(id.clone());
    }
    // 2. Case-insensitive exact match
    let lower = name.to_lowercase();
    if let Some((_, id)) = known_chars.iter().find(|(n, _)| n.to_lowercase() == lower) {
        return Some(id.clone());
    }
    // 3. First-name match — e.g., "Roran" matches "Roran Ironfist"
    if let Some((_, id)) = known_chars.iter().find(|(n, _)| {
        n.split_whitespace()
            .next()
            .map(|first| first.to_lowercase() == lower)
            .unwrap_or(false)
    }) {
        return Some(id.clone());
    }
    // 4. Word-boundary match — e.g., "Shadowcloak" matches "Finn
    // Shadowcloak" (a last name / name fragment). Matches on a shared WHOLE
    // WORD, not an arbitrary substring — a raw `.contains()` here used to
    // let a short new name like "Ari" match an unrelated existing "Aria
    // Silverleaf" (since "aria silverleaf" contains "ari" as a substring),
    // silently misattributing a brand-new speaker's dialogue to the wrong,
    // already-established character.
    let candidate_words: std::collections::HashSet<&str> = lower.split_whitespace().collect();
    known_chars
        .iter()
        .find(|(n, _)| n.to_lowercase().split_whitespace().any(|w| candidate_words.contains(w)))
        .map(|(_, id)| id.clone())
}
