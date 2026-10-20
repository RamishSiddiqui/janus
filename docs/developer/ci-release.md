# CI and releases

## Pull request checks

| Workflow | What it runs |
|---|---|
| **CI** (`ci.yml`) | **Frontend:** `npm ci`, `npm run check`. **Backend:** installs the Tauri Linux dependencies, builds the frontend, then `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `cargo test`. |
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
3. creates a draft GitHub release with every installer attached;
4. publishes it.

The changelog is used only for the release notes; it is not committed back to `master`.

Before tagging, bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` together, and update version references in the docs.

### Commit messages drive the notes

Use `type: short description` (`feat`, `fix`, `refactor`, `docs`, `build`, `test`). The changelog template renders only each commit's **subject line**, so put issue references there, for example `fix: message delete undo (#20)`. A reference only in the body never appears in the release notes.

## Installers are unsigned

Releases are not code-signed yet, so Windows SmartScreen and macOS Gatekeeper warn on first launch. The README explains the one-time steps for users.
