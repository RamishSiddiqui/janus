# Accounts and ownership

Janus supports several accounts on one installation (issue #94). Each account owns its own data; the admin controls who may sign up.

## Model

- Table `users`: `username` (unique, lowercase), role `admin` or `member`, argon2id `passphrase_hash`, a hashed recovery key, failed-attempt counter and lockout time, `must_change` (set when an admin created the account with a temporary passphrase), timestamps.
- Table `sessions`: the **SHA-256 hash** of a random session token, user, device label, `trusted`, last seen, expiry.
- Table `auth_settings` (single row `main`): `signup_mode` (`open` or `admin_only`) and the network settings (`network_enabled`, `network_lan`, `network_port`, `network_https`).
- The **first** account created is the admin. Nobody has to configure anything.

Rules (in `auth/mod.rs` and `auth/service.rs`):

| Rule | Value |
|---|---|
| Username | 3 to 24 characters of letters, digits, `.`, `_`, `-`, starting with a letter or digit. Stored lowercase. |
| Passphrase | 12 to 256 characters. Stored only as an argon2id hash. |
| Lockout | 3 wrong passphrases in a row from the same place (a browser address, or the desktop) lock that place out of that account for 300 seconds. Kept in memory per (username, place), and also for names that do not exist, so a sign-in answers the same way whether or not the account exists, and the message never says how many tries are left. A stranger cannot lock the owner out. 20 wrong passphrases in a row from anywhere lock the account itself for 300 seconds. |
| Session | 30 days if "keep this device signed in", otherwise 12 hours. |
| Recovery key | `JANUS-` plus 16 characters from an unambiguous alphabet, shown once, stored hashed. Using it sets a new passphrase and signs out all other sessions. |

## Ownership

Seven **root tables** have an `owner_id` string: `characters`, `personas`, `conversations`, `provider_configs`, `image_presets`, `lorebook_entries`, `memories`. Everything else is a **child** and inherits through its root: messages, scenes, scene states, character states, summaries, embeddings and cast rows through their conversation; models through their provider.

`owner_id = ''` means "belongs to nobody yet". The first account to register **claims every unowned row**, so existing data is not lost. Migration 3 backfills `''` on rows that predate accounts.

Before any account exists the app runs in **legacy mode**: `Actor::Legacy`, nothing filtered, exactly as before accounts existed.

## `Actor` and the access helpers

`auth/access.rs`:

```rust
pub enum Actor { Legacy, User(UserInfo) }
```

- `commands::actor::acting(&state)` returns `(db, actor)`. A browser request carries its account in the task-local `web::rpc::REQUEST_USER`; otherwise the desktop's `desktop_user` is used.
- `ensure_character`, `ensure_persona`, `ensure_conversation`, `ensure_provider`, ... check that an id the client sent belongs to the actor. **A missing row and someone else's row both return `NotFound`**, so existence never leaks.
- `ensure_file_access(db, actor, relative)` allows a stored file only if one of the actor's rows references it (avatar, portrait, scene file, message attachment).
- `ensure_attachment_path` accepts a plain `attachments/<file>` path that is unreferenced or the actor's own.
- `stamp_owner(db, actor, table, id)` marks a row the actor just created.
- `inherit_owner_from_conversation` stamps rows created by background pipelines.
- `owner_filter_for_conversation` and `owner_filter_for_row` give background work (retrieval, summaries) the owner to filter by.

**Writing a new command:** call `acting`, then for every id from the client call the matching `ensure_*`; for every list filter by `actor.owner()`; for every row you create call `stamp_owner`. The isolation tests (`tests/auth_isolation.rs`) show the expected behaviour and should be extended for new data.

## The desktop and accounts

- The desktop acts as `AppState.desktop_user`: the admin by default. Signing out clears it; signing in sets it.
- `auth_switch_to` (desktop only) lets the admin act as any account without its passphrase, remembering the admin in `desktop_admin` so "return to admin" also needs no passphrase. It cannot be reached from a browser.
- Admin-only actions call `actor.require_admin()`: backups, logs, network settings, account management.
- Deleting an account (`auth_delete_user`) collects the account's file paths first (`OwnershipRepo::files_of_owner`), deletes its rows, then removes the files.

## Sessions over HTTP

The browser holds the token in an `HttpOnly; SameSite=Strict; Path=/` cookie it cannot read. `web::util::AuthedUser` resolves it with `auth::service::authenticate`; `ActiveUser` additionally refuses an account that still has `must_change` set. The cookie gets `Secure` only when the server itself speaks HTTPS (`WebState::https`); on plain HTTP a browser would refuse to store it, which is why the docs limit plain-HTTP browser access to trusted networks.

See {doc}`browser-server` for the HTTP side and the {doc}`/reference/http-api`.
