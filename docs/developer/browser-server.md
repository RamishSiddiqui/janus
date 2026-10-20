# Browser server and bridge

Janus can serve itself to browsers on the local network (issue #94 / #14). This page covers both halves: the Rust server (`src-tauri/src/web/`) and the frontend bridge (`src/lib/services/webBridge.ts`). The endpoints are in the {doc}`/reference/http-api`.

## Server

`web::WebServer` is an axum server started by `commands::web::apply_settings` when the admin turns browser access on (and again at startup if it was left on).

- **Bind address:** `127.0.0.1` unless LAN access is on, then `0.0.0.0`. The default port is stored in `auth_settings`; the port must be 1024 or higher. If binding fails, browser access is switched back off and the error is returned.
- **Static files:** the built frontend, looked up through an `AssetFn` over Tauri's embedded assets. Unknown route-like paths return `index.html` so the SPA can route; a missing real file (one with an extension) is a 404.
- **Layers** (outermost first): `security_headers` (`X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store` on `/api`), then `csrf_guard`, then a 64 KB default body limit (raised to 12 MB on the image upload routes).
- **Rate limiting:** sign-up, sign-in and recovery allow 10 attempts per minute per client address, on top of the per-account lockout.

### Request guards

- `csrf_guard`: any request other than `GET`, `HEAD` and `OPTIONS` must carry the header `x-janus-request`, and if an `Origin` header is present its host must equal the request's `Host`. Browsers cannot add that header cross-site without a CORS preflight the server never grants, so cross-site forms and fetches are rejected.
- `AuthedUser` resolves the session cookie to an account.
- `ActiveUser` also refuses an account whose `must_change` is set.
- `ClientIp` reads the peer address for the rate limiter.

## Calling commands: `/api/rpc/{name}`

`web/rpc.rs` runs the **same command functions the desktop runs**.

1. The handler authenticates the session.
2. It runs `rpc.call(name, args)` inside `REQUEST_USER.scope(user_id, ...)`. `commands::actor::acting` reads that task-local, so every ownership check written for the desktop applies unchanged.
3. `AppRpc` dispatches through `rpc_tables`, which tries each family (`rpc_characters`, `rpc_conversations`, `rpc_providers`, `rpc_chat`) in turn.

A family lists commands in a macro:

```rust
crate::rpc_family! {
    "get_character" => ch::get_character [state] (id: String);
    "list_characters" => ch::list_characters [state] ();
}
```

The bracket lists injected parameters (`state`, `app`), the parentheses the JSON arguments (camelCase on the wire). `rpc_family!` is generic over the Tauri runtime so tests can drive it with a mock app; `rpc_family_wry!` is for commands that need the real window's `AppHandle`.

**What is deliberately not exposed:** commands that read or write paths on the host (file pickers, imports from a path, backups, log access), host-global operations (voice model downloads, network settings, account switching), and anything returning an absolute host path. The browser gets purpose-built endpoints for files instead. The {doc}`/reference/commands` marks each command **Browser** or not.

To expose a command: add it to the right family table, then (if it takes a file) decide whether it needs an upload endpoint.

## Files: `/api/files` and uploads

- `GET /api/files/{path}`: serves a stored file only if `ensure_file_access` passes for the signed-in account, the path resolves inside the data directory (`resolve_within`), and the extension is one of png, jpg, jpeg, webp, gif, mp4, webm, wav, mp3, ogg. Responses are `Cache-Control: private`.
- `POST /api/upload/attachment?ext=png`: raw image bytes; shares `write_attachment_in` with the desktop command.
- `POST /api/upload/portrait/{character_id}`: sets a character's portrait; shares `store_portrait`.
- `POST /api/upload/character-card` and `/api/upload/persona-card`: a card PNG; share `import_character_png` and `import_persona_png` with the desktop import commands.

Uploads are capped at 12 MB.

## Events: `/api/events`

A Server-Sent Events stream. Each event is a named SSE event whose data is the JSON payload. See {doc}`events`.

## The frontend bridge

When the page is served by Janus (not by Tauri), `hooks.client.ts` runs `initWebMode()` before the app renders:

1. `detectWebMode()` calls `/api/auth/status`. A plain dev server answers with HTML, so it is not mistaken for Janus.
2. `installBridge()` defines a stand-in for `window.__TAURI_INTERNALS__`: `invoke`, `transformCallback`, `unregisterCallback`, `convertFileSrc`, and window metadata, plus `__TAURI_EVENT_PLUGIN_INTERNALS__`.

That is enough for `@tauri-apps/api` to work unchanged:

| Tauri call | In a browser |
|---|---|
| `invoke(cmd, args)` | `POST /api/rpc/{cmd}` with the JSON args. A non-2xx response rejects with `{ error, message }`. |
| `listen(event, cb)` | One shared `EventSource('/api/events')`; handlers get `{ event, id, payload }` like Tauri. |
| `plugin:dialog|open` | The browser's own file picker; returns a token for the chosen `File`. |
| commands taking a `filePath` | `upload_message_attachment`, `upload_character_avatar`, `import_character_card`, `import_persona_card` upload the picked file's bytes instead. |
| `upload_message_attachment_bytes` | `POST /api/upload/attachment`. |
| `plugin:fs|stat` | The picked file's size. |
| other `plugin:*` calls | Rejected with `{ error: 'unsupported' }`. |
| `loadFileAsBlobUrl(path)` | `GET /api/files/{path}` as a blob. |

A 401 from any call raises the `janus:signed-out` window event; `+layout.svelte` listens and shows the sign-in gate again (`WebGate.svelte`). The account calls used by the gate are in `webAuth` in the same file.

`isWebMode()` lets shared code branch when it must (the title bar shows the account and Sign out instead of window buttons; the Network Access settings section is hidden).

## Security properties

- Passphrases are argon2id hashed; sessions are random tokens stored as SHA-256 hashes; recovery keys are stored hashed.
- The session cookie is `HttpOnly; SameSite=Strict`. It cannot be `Secure` on plain HTTP.
- Ownership checks are the same ones the desktop uses; a missing row and another account's row are indistinguishable.
- Provider API keys never leave the host to a browser except as the owner's own provider records.
- Plain HTTP means anyone who can watch the network can read traffic. The product guidance is a trusted home network only; there is no TLS.

## Tests

`tests/web_auth.rs` drives the real router in-process (no sockets) against a real database: sign-up and cookie flags, CSRF and origin checks, lockout, signup modes, the RPC layer with a mock app, event delivery per account, file ownership, size limits and uploads. `webBridge.test.ts` covers the bridge.
