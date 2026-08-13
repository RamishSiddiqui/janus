/**
 * Case-insensitive {{user}} -> display-name substitution, mirroring the
 * Rust-side `substitute_user_macro` used in `build_prompt`. Used only for
 * the client-seeded character greeting (`first_mes`), which never passes
 * through the backend prompt pipeline. Uses `personaName` when given,
 * otherwise falls back to the generic "User" token — same fallback as the
 * backend, so {{user}}-authored cards read naturally even with no persona
 * selected.
 */
export function substituteUserMacro(text: string, personaName: string | null | undefined): string {
  return text.replace(/\{\{user\}\}/gi, personaName || 'User');
}
