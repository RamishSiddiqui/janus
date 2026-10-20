# Testing

## Backend (`src-tauri`)

```bash
cd src-tauri
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

CI runs all three. A clippy warning or a formatting difference fails the build, so run them before pushing.

### Unit tests

Beside the code under `#[cfg(test)]`: token budget and window, the multi-character parser, the sentence chunker, rank fusion, auth helpers, the migration default parser, and others.

### Integration tests (`src-tauri/tests/`)

They use the real embedded database in a temporary folder (`init_database(&dir)`), so they exercise real queries, schema and migrations.

| File | Covers |
|---|---|
| `auth_service.rs` | Sign-up, sign-in, lockout, sessions, recovery, signup modes. |
| `auth_isolation.rs` | Two accounts cannot see or touch each other's data: every `ensure_*` check, owner stamping, owner-filtered lists, file access, account deletion. |
| `web_auth.rs` | The browser server driven in-process through the real router: auth endpoints and cookie flags, CSRF and origin checks, rate limits, the RPC layer (with a mock Tauri app), event delivery per account, file ownership, uploads and size limits, card imports. |
| `migrations_backfill.rs` | A legacy row (missing defaulted fields) does not block the account backfill. |
| `hybrid_search.rs`, `vector_search.rs` | Retrieval against a real database. |
| `bindings_export.rs` | The specta registry exports valid TypeScript. |

Command bodies need a Tauri runtime, so tests exercise what commands call (the repos and access checks), and the browser RPC path is tested with Tauri's mock runtime (`tauri` with the `test` feature as a dev dependency).

### Writing an isolation test for new data

When you add a table or a command that reads user data, extend `auth_isolation.rs`: create the data as one account, then prove another account gets `NotFound` from the `ensure_*` check and an empty list from the filtered query.

### Pitfalls

- Seed data in a debug build is claimed by the first account that registers. Compare against a baseline count instead of asserting an absolute number.
- Do not put real user identifiers in assertion messages; code scanners flag them.

## Frontend

```bash
npm test            # vitest run
npm run check       # svelte-check
```

Vitest runs `src/**/*.test.ts`. Current suites: `chatEmotion`, `onboarding` (model ranking, wizard conditions, starter characters), `webAccount` (passphrase strength, usernames, recovery-key typing, time labels), and `webBridge` (the browser transport: commands, errors, sign-out signal, events, files, pickers). Keep logic that can be pure in `lib/utils/` so it can be tested without a DOM.

## Manual checks

For changes to the browser path, run `npm run tauri dev`, enable browser access, and walk through sign-in, a chat, the gallery and an upload in a real browser. For phone layout, use the browser's device toolbar or a phone on the same network.
