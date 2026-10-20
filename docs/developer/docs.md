# Documentation

These docs are a [Sphinx](https://www.sphinx-doc.org/) project with the [Furo](https://pradyunsg.me/furo/) theme and [MyST](https://myst-parser.readthedocs.io/) for Markdown. They live in `docs/`.

```text
docs/
├── conf.py              Sphinx configuration
├── index.md             landing page and top-level navigation
├── requirements.txt     Python dependencies
├── user/                hand-written user guide
├── developer/           hand-written developer guide
├── reference/           GENERATED from the source (do not edit)
├── tools/
│   ├── gen_reference.py the generator and coverage checks
│   └── summaries.json   one-line summaries for files without a leading comment
└── _static/
```

## Building

```bash
python -m venv .venv && . .venv/bin/activate      # Windows: .venv\Scripts\activate
pip install -r docs/requirements.txt
python docs/tools/gen_reference.py                 # refresh the reference pages
python -m sphinx -W --keep-going -b html docs docs/_build/html
```

Open `docs/_build/html/index.html`. `-W` turns warnings (such as a broken link or reference) into errors, as CI does.

## The generated reference

`gen_reference.py` reads the source and writes `docs/reference/*.md`:

| Page | Source |
|---|---|
| `commands.md` | Every `#[tauri::command]`: its `///` doc comment, arguments, return type, module, and whether a browser may call it (from `web/rpc_*.rs`). |
| `events.md` | `emit_scoped(...)` call sites and `listen(...)` call sites. |
| `settings.md` | The `AppSettings` interface and defaults in `stores/settings.ts`. |
| `database.md` | `db/schema.rs` tables, fields and indexes, and `db/migrations.rs`. |
| `http-api.md` | The routes in `web/` with their handlers' doc comments. |
| `source-map.md` | The leading comment of every source file. |

Commit the regenerated files with your change. CI runs `python docs/tools/gen_reference.py --check` and fails if they differ.

## What the check enforces

The same check makes documentation coverage a build requirement. It fails when:

- a `#[tauri::command]` has no `///` description;
- a command is missing from either registration list in `lib.rs`, or a list names something that is not a command;
- an HTTP handler has no description;
- a setting in `AppSettings` has no doc comment;
- a source file (Rust, TypeScript or Svelte, including tests) has neither a leading comment nor an entry in `docs/tools/summaries.json`;
- `summaries.json` names a file that no longer exists.

So adding a command, an endpoint, a setting or a file means documenting it in the same change. For a new file, write a one-line summary comment at the top (`//!` in Rust, a comment block in TypeScript or a leading comment in the Svelte script) or add an entry to `summaries.json`.

## Writing the guides

- Write for the reader's task, in plain words. Use the names the UI shows, in bold (for example **Settings, Chat Behavior**).
- State what is not available yet instead of implying it works.
- Cross-link with `{doc}` roles and relative Markdown links. The build fails on a broken one.
- When you change behaviour, update the guide in the same pull request.
