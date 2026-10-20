# Developer guide

For people who want to understand, change or extend Janus. Start with {doc}`setup` to run it, then {doc}`architecture` for the big picture.

## Orientation

| Page | Covers |
|---|---|
| {doc}`setup` | Prerequisites, running, building, and everyday commands. |
| {doc}`architecture` | The two halves, design decisions, a request end to end, technology. |
| {doc}`repository-layout` | Where everything lives. |
| {doc}`backend` | Startup, `AppState`, commands, errors, concurrency, logging, backups. |
| {doc}`frontend` | IPC, state, stores, routes, components, conventions, responsive layout. |

## How it works

| Page | Covers |
|---|---|
| {doc}`chat-pipeline` | From a sent message to a streamed reply and the work after it. |
| {doc}`context-engine` | Prompt layers, the token budget, the window, summaries and hybrid retrieval. |
| {doc}`npc-pipeline` | Automatic NPC detection and profile generation. |
| {doc}`providers` | LLM, image/video and voice adapters, and how to add one. |
| {doc}`tts` | The built-in Kokoro engine and streamed synthesis. |
| {doc}`database` | SurrealDB conventions, gotchas and migrations. |
| {doc}`accounts` | Accounts, sessions and the ownership model. |
| {doc}`browser-server` | The HTTP server, the RPC layer and the frontend bridge. |
| {doc}`events` | Event routing to the right account. |

## Working on Janus

| Page | Covers |
|---|---|
| {doc}`testing` | Backend and frontend tests and what each suite covers. |
| {doc}`ci-release` | The CI checks, dependency updates and how a release is made. |
| {doc}`contributing` | The workflow, the path for a new feature, and conventions. |
| {doc}`docs` | How these docs are built and kept in sync with the code. |

## Reference (generated)

{doc}`/reference/commands`, {doc}`/reference/events`, {doc}`/reference/settings`, {doc}`/reference/database`, {doc}`/reference/http-api` and {doc}`/reference/source-map` are produced from the source.

```{toctree}
:hidden:

setup
architecture
repository-layout
backend
frontend
chat-pipeline
context-engine
npc-pipeline
providers
tts
database
accounts
browser-server
events
testing
ci-release
contributing
docs
```
