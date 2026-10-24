# CI and releases

## Pull request checks

| Workflow | What it runs |
|---|---|
| **CI** (`ci.yml`) | **Frontend:** `npm ci`, `npm run check`, `npm run lint`, `npm run format:check`, `npm test`. **Backend:** `cargo fmt --check` first (it fails in seconds), then installs the Tauri Linux dependencies, builds the frontend, and runs `cargo clippy --all-targets -- -D warnings` and `cargo test`. |
| **CodeQL** (`codeql.yml`) | Static analysis for JavaScript/TypeScript and Rust, on pushes, pull requests and weekly. |
| **Gitleaks** (`gitleaks.yml`) | Secret scanning. |
| **Security audit** (`security-audit.yml`) | `cargo audit` and `npm audit --audit-level=high`, weekly and whenever a dependency file changes. Known, justified advisories are listed with reasons in the workflow. |
| **Docs** (`docs.yml`) | Regenerates the reference pages and fails if they differ; builds the docs with Sphinx and warnings as errors; uploads the HTML. |

`master` accepts changes only through pull requests. Merge only when every check has finished and passed. A check reported as "skipped" or "neutral" is not a pass; read the individual results, and in particular CodeQL.

## Dependency updates

Dependabot opens pull requests. Breaking major bumps (for example a new `rig-core` minor) are deferred until someone can port the code. `npm audit fix` is the usual remedy for a new advisory in a transitive frontend dependency.

## Releasing

Releases are automatic. Push a version tag:

```bash
git tag v0.1.3
git push origin v0.1.3
```

`.github/workflows/release.yml` then:

1. generates release notes with [git-cliff](https://git-cliff.org/) from the commits since the last tag (template in `cliff.toml`);
2. builds installers in a matrix: macOS (Apple Silicon and Intel), Windows, Linux (`.deb`, `.AppImage`, `.rpm`);
3. signs the update packages and creates a draft GitHub release with every installer and `latest.json` attached;
4. publishes it.

The changelog is used only for the release notes; it is not committed back to `master`.

Before tagging, bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` together, and update version references in the docs.

### Commit messages drive the notes

Use `type: short description` (`feat`, `fix`, `refactor`, `docs`, `build`, `test`). The changelog template renders only each commit's **subject line**, so put issue references there, for example `fix: message delete undo (#20)`. A reference only in the body never appears in the release notes.

## In-app updates

The desktop app (`tauri-plugin-updater`, wired in `lib.rs`, with `updater:default` and `process:allow-restart` in `capabilities/default.json`) reads `https://github.com/RamishSiddiqui/janus/releases/latest/download/latest.json`, which `tauri-action` uploads with each release (`uploadUpdaterJson: true`). The frontend side is `src/lib/stores/updates.ts` (state, checks, install, restart), `UpdatePrompt.svelte` (the corner card) and `SettingsUpdatesSection.svelte`. Browsers, dev builds and users who turned the setting off never check automatically.

**Signing.** Update packages are signed with a minisign key pair made by `npx tauri signer generate`. The **public** key is `plugins.updater.pubkey` in `tauri.conf.json`; the **private** key and its password are repository secrets used by the release job:

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | contents of the private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password |

Keep a backup of the private key. If it is lost, installed copies cannot verify new updates and everyone must reinstall by hand; if it leaks, rotate the pair and ship a release signed with the old key that carries the new public key. `createUpdaterArtifacts` is switched on only for releases, by `src-tauri/tauri.release.conf.json` (passed with `--config` in the workflow), so an ordinary `tauri build` needs no key. Without the secrets the release build fails.

**First updater release.** Versions released before this feature (v0.1.2 and earlier) have no updater, so users install the first one that has it manually. To test the whole flow, release a version, install it, then tag a higher one and watch the first install offer it.

**For contributors.** `npm run tauri dev` never checks or installs updates: **Settings, Updates, Check now** answers "Updates are turned off in development builds", so a branch based on an older version cannot replace itself with a release. To test the update flow in dev, start it with `VITE_JANUS_UPDATES=1` (the steps below). A branch cut before this feature just picks it up when rebased onto `master`; if `Cargo.lock` or `package-lock.json` conflict, take `master`'s and rerun `npm install` and `cargo check`. A local `tauri build` makes installers that update from the official feed and replace themselves with the official build when asked. If you distribute your own build (a fork), change `plugins.updater.endpoints` and generate your own key pair for `pubkey`. To try the update UI end to end, point `endpoints` at a `latest.json` you host (a `python -m http.server` is enough), sign a higher-versioned build with your own key, and use your own public key.

Bumping the version in the three files (see above) is what makes an update available: the check compares the installed version with the one in `latest.json`.

## Installers are unsigned

Releases are not OS code-signed yet (the update signature above is separate and only covers in-app updates), so Windows SmartScreen and macOS Gatekeeper warn on first launch. The README explains the one-time steps for users.
