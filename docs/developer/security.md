# Security and review

Janus asks people to trust it with their conversations and ships signed updates to their computers. This page explains how the repository protects that trust, and what every contributor and maintainer is expected to do.

## What the app promises

Janus talks to **only** two kinds of place:

- the AI, image, video and voice **providers the person added** (their own base URLs and keys), and
- **GitHub**, to look for a newer release (this can be switched off in Settings, Updates).

There is no telemetry, analytics, crash reporting or cloud sync. Any change that makes the app contact somewhere new breaks that promise, so it is checked automatically and by a person (see below).

## How a change reaches users

1. **Fork and pull request.** Nobody but the maintainers pushes to `master`. Contributors work in forks.
2. **Branch rules.** `master` requires a pull request, passing checks and an approving review, and blocks force-pushes and deletion. Only a maintainer can create a `v*` tag.
3. **Code owners.** [`CODEOWNERS`](https://github.com/RamishSiddiqui/janus/blob/master/.github/CODEOWNERS) lists the paths that always need the owner's review: workflows, release and updater settings, `capabilities/` (what the app may do), build scripts, dependency files, and the account and browser-server code.
4. **Automatic checks** on every pull request:

| Check | Catches |
|---|---|
| Backend, Frontend, Docs builds | Broken code, and documentation that has fallen behind. |
| CodeQL (Rust, TypeScript) | Common vulnerability patterns. |
| gitleaks and push protection | Committed secrets. |
| Security audit (`cargo audit`, `npm audit`) | Known-vulnerable dependencies already in the lockfiles. |
| Dependency review and `cargo-deny` | A change that *adds* a vulnerable dependency, or a crate from outside crates.io. |
| Network destinations | A change that adds a URL or host the code did not use before. |
| CLA | A contributor who has not agreed to [`CLA.md`](https://github.com/RamishSiddiqui/janus/blob/master/CLA.md). |

5. **Release.** Pushing a `v*` tag starts the release workflow in a protected `release` environment that a maintainer must approve. It builds the installers, **signs the update packages** with a key only that environment can read, publishes `SHA256SUMS` and a **build-provenance attestation**, and then publishes the release.

## Reviewing a pull request

Read the diff as if it came from a stranger. Slow down for:

- **New network destinations.** The Network destinations check lists any new host. Confirm it is needed and that nothing personal is sent. When satisfied, add the `network-reviewed` label.
- **Dependencies.** A new crate or package, a changed version, a `build.rs` or install script, or a lockfile change that the manifest does not explain. Look the package up: its age, its maintainers, its download count, and whether the name is a near-copy of a popular one.
- **`capabilities/`, `tauri.conf.json` and the CSP.** These decide what the app may read, write, run and connect to. A widened permission needs a reason in the pull request.
- **Obfuscation.** Minified or encoded blobs, `eval`, dynamic code loading, base64 strings that are decoded and executed, or code that behaves differently for certain users or dates.
- **Workflows.** A new `pull_request_target`, a step that prints or sends secrets, or an Action that is not pinned to a commit.
- **Auth and ownership.** Anything that reads or writes data without `acting()` and the `ensure_*` ownership checks ({doc}`accounts`).

Squash-merge, so the history stays one reviewed commit per change.

## Rules for workflows

- Every `uses:` is pinned to a **full commit SHA** with the version in a comment. Dependabot updates them; review those pull requests like any other.
- Workflows ask for the least permission they need (`permissions:` at the top, read-only by default).
- Fork pull requests run without secrets. Never use `pull_request_target` to run code from a fork.
- Secrets used for releases live in the `release` environment, not in the repository's secrets.

## Maintainer account and keys

- Turn on two-factor authentication with a hardware key or passkey, and keep recovery codes offline.
- Keep a second owner on the repository or organisation so losing one account does not lose the project.
- The **update signing key** is the most valuable secret: a copy of it can push code to every installed Janus. Keep an offline backup, never put it in the repository or a chat, and rotate it if it might have leaked ({doc}`ci-release`).
- Report and fix vulnerabilities privately, through GitHub's private vulnerability reporting (see `SECURITY.md`).

## Reporting something suspicious

If you find code in a pull request, a dependency or a release that looks malicious, do not discuss it in a public issue. Report it through the private route in `SECURITY.md`.
