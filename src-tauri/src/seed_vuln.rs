//! Throwaway: a deliberate path-injection finding to compare CodeQL modes.
pub fn read_user_chosen_file() -> std::io::Result<String> {
    let path = std::env::args().nth(1).unwrap_or_default();
    std::fs::read_to_string(path)
}
