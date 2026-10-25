# CI and releases

## Pull request checks

| Workflow | What it runs |
|---|---|
| **CI** (`ci.yml`) | **Frontend:** `npm ci`, `npm run check`, `npm run lint`, `npm run format:check`, `npm test`. **Backend:** `cargo fmt --check` first (it fails in seconds), then installs the Tauri Linux dependencies, builds the frontend, checks that every Tauri plugin's npm package and Rust crate are on the same minor version (`tauri build` refuses to run otherwise), and runs `cargo clippy --all-targets -- -D warnings` and `cargo test`. |
| **CodeQL** (`codeql.yml`) | Static analysis for JavaScript/TypeScript and Rust, on pushes, pull requests and weekly. |
| **Gitleaks** (`gitleaks.yml`) | Secret scanning. |
| **Security audit** (`security-audit.yml`) | `cargo audit` and `npm audit --audit-level=high`, weekly and whenever a dependency file changes. Known, justified advisories are listed with reasons in the workflow. |
| **Docs** (`docs.yml`) | Regenerates the reference pages and fails if they differ; builds the docs with Sphinx and warnings as errors; uploads the HTML. |
| **Release dry run** (`release-dry-run.yml`) | Builds the installers the way a release does, with no secrets and no publishing. Weekly on Linux, Windows and macOS, on demand, on all platforms when such a change merges to `master`, and on Linux for pull requests that touch the Tauri packages, config or release workflow. The `master` run also saves the Rust build cache that the release restores (a tag build can only read caches saved on `master`), so merging the version bump is what makes the release build fast. Not a required check, because path-filtered checks cannot be. |

`master` accepts changes only through pull requests. Merge only when every check has finished and passed. A check reported as "skipped" or "neutral" is not a pass; read the individual results, and in particular CodeQL.

## Dependency updates

Dependabot opens pull requests. Breaking major bumps (for example a new `rig-core` minor) are deferred until someone can port the code. `npm audit fix` is the usual remedy for a new advisory in a transitive frontend dependency.

## Releasing

A release is a version tag, but a few steps come before it and one after.

1. **Bump the version** in one pull request: `package.json`, `package-lock.json` (two places), `src-tauri/Cargo.toml`, the `janus` entry in `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`. Update version references in the docs. Merge it with every check green.
2. **Rehearse the release build.** Open **Actions, Release dry run, Run workflow** on `master` and wait for all three platforms to pass. Do this whenever dependencies changed since the last release, and it is cheap enough to do every time. CI proves the code compiles and passes tests, but only a bundling build proves the release itself works.
3. **Tag the bump commit and push the tag:**

   ```bash
   git tag v0.1.7 <bump commit>
   git push origin v0.1.7
   ```

4. **A maintainer approves the `release` deployment** on the workflow run (Review deployments). The signing key lives in that environment and nothing builds until someone approves.

`.github/workflows/release.yml` then:

1. generates release notes with [git-cliff](https://git-cliff.org/) from the commits since the last tag (template in `cliff.toml`);
2. builds installers in a matrix: macOS (Apple Silicon and Intel), Windows, Linux (`.deb`, `.AppImage`, `.rpm`);
3. signs the update packages and creates a draft GitHub release with every installer and `latest.json` attached;
4. only if **every** build succeeded: adds `SHA256SUMS`, records a build attestation and publishes the release.

Finally, **verify** the published release: all installers and `latest.json` are present, `latest.json` carries the new version, and `gh attestation verify <file> --repo RamishSiddiqui/janus` passes for a downloaded installer.

The changelog is used only for the release notes; it is not committed back to `master`.

### When a release fails

- **A pushed tag is never moved or deleted.** Fix the cause in a pull request, bump to the next patch version and tag that. SemVer says a released version must never change, and Git advises against rewriting published tags. A failed tag simply has no release attached.
- **A failed run publishes nothing.** The publish job needs all four builds, so there is no partial release, no `latest.json` and no update offered. Installed copies stay on the previous version.
- **Write it up.** A short blameless note on the tracking issue: what happened, why nothing caught it earlier, and what now prevents it. The aim is a guard or a check, not a reminder to be careful. The v0.1.6 failure (a Tauri plugin's npm package and crate on different minor versions) produced the check in `ci.yml` and the dry-run workflow above.

### Commit messages drive the notes

Use `type: short description` (`feat`, `fix`, `refactor`, `docs`, `build`, `test`). The changelog template renders only each commit's **subject line**, so put issue references there, for example `fix: message delete undo (#20)`. A reference only in the body never appears in the release notes.

## In-app updates

The desktop app (`tauri-plugin-updater`, wired in `lib.rs`, with `updater:default` and `process:allow-restart` in `capabilities/default.json`) reads `https://github.com/RamishSiddiqui/janus/releases/latest/download/latest.json`, which `tauri-action` uploads with each release (`uploadUpdaterJson: true`). The frontend side is `src/lib/stores/updates.ts` (state, checks, install, restart), `UpdatePrompt.svelte` (the corner card) and `SettingsUpdatesSection.svelte`. Browsers, dev builds and users who turned the setting off never check automatically.

**Signing.** Update packages are signed with a minisign key pair made by `npx tauri signer generate`. The **public** key is `plugins.updater.pubkey` in `tauri.conf.json`; the **private** key and its password are secrets of the `release` environment (not repository secrets), so only a build a maintainer approved can read them:

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | contents of the private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password |

Keep a backup of the private key. If it is lost, installed copies cannot verify new updates and everyone must reinstall by hand; if it leaks, rotate the pair and ship a release signed with the old key that carries the new public key. `createUpdaterArtifacts` is switched on only for releases, by `src-tauri/tauri.release.conf.json` (passed with `--config` in the workflow), so an ordinary `tauri build` needs no key. Without the secrets the release build fails.

**First updater release.** Versions released before this feature (v0.1.2 and earlier) have no updater, so users install the first one that has it manually. To test the whole flow, release a version, install it, then tag a higher one and watch the first install offer it.

**For contributors.** `npm run tauri dev` never checks or installs updates: **Settings, Updates, Check now** answers "Updates are turned off in development builds", so a branch based on an older version cannot replace itself with a release. To test the update flow in dev, start it with `VITE_JANUS_UPDATES=1` (the steps below). A branch cut before this feature just picks it up when rebased onto `master`; if `Cargo.lock` or `package-lock.json` conflict, take `master`'s and rerun `npm install` and `cargo check`. A local `tauri build` makes installers that update from the official feed and replace themselves with the official build when asked. If you distribute your own build (a fork), change `plugins.updater.endpoints` and generate your own key pair for `pubkey`. To try the update UI end to end, point `endpoints` at a `latest.json` you host (a `python -m http.server` is enough), sign a higher-versioned build with your own key, and use your own public key.

Bumping the version in the files listed under Releasing is what makes an update available: the check compares the installed version with the one in `latest.json`.

## Hosted documentation

These docs are published on [Read the Docs](https://readthedocs.org/) from `.readthedocs.yaml` at the repository root (Ubuntu 24.04, Python 3.13, `docs/conf.py`, `docs/requirements.txt`). It is separate from GitHub Actions: Read the Docs builds on its own servers from a webhook it installs on the repository, and it builds only what is committed. It does not run `docs/tools/gen_reference.py`, so the generated reference pages must be committed up to date; the Docs workflow enforces that.

**What triggers a build.** A push to `master` rebuilds `latest`. Pushing a version tag, the same push that starts a release, creates a docs version for that tag and builds it. Pull requests get a preview build and a status check when *Build pull requests for this project* is on.

**Versions.**

| Version | Shows |
|---|---|
| `latest` | The head of `master`, including features that are not released yet. |
| `stable` | Created automatically; points at the highest version tag, so it moves forward with each release. |
| `v0.1.7`, `v0.1.8`, ... | A frozen snapshot of the docs at that tag. |

Tags made before `.readthedocs.yaml` was merged (up to v0.1.7) cannot build, because Read the Docs reads the config from the tagged commit. Leave those versions inactive; the first docs version is the first tag cut after the file landed.

**One-time setup, by a maintainer on readthedocs.org:**

1. Sign in with GitHub, *Import a Project*, and choose this repository.
2. *Advanced settings*: turn on *Build pull requests for this project*.
3. *Automation rules*: add a rule that activates versions matching *SemVer versions*, so each release tag appears without a manual step.
4. Until the first tag that contains `.readthedocs.yaml` has built (v0.1.8 or later), keep the default version on `latest`. After that, set it to `stable`, so the front page shows released docs.
5. Put the docs URL in the README and as the repository homepage.

## Installers are unsigned

Releases are not OS code-signed yet (the update signature above is separate and only covers in-app updates), so Windows SmartScreen and macOS Gatekeeper warn on first launch. The README explains the one-time steps for users.
