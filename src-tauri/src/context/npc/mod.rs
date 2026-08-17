//! Story-driven NPC detection: notices when a new character becomes
//! narratively significant enough to track, generates a backstory-aligned
//! profile for them once confirmed, and adds them to the conversation's
//! cast as an NPC — separate from `scene_extractor`'s mechanical scene-state
//! tracking, since detecting narrative significance is a different judgment
//! call with a different failure-mode blast radius (a bad call here creates
//! an unwanted character; a bad call there just mislabels a location).

pub mod detector;
pub mod pipeline;
pub mod profile_generator;
