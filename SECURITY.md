# Security Policy

## Reporting a Vulnerability

If you find a security vulnerability in Janus, please **do not open a public issue**.

Instead:

1. Use GitHub's [private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing/privately-reporting-a-security-vulnerability) (Security tab → "Report a vulnerability") on this repository, if enabled, **or**
2. Email **ramishsiddique46@gmail.com** with a description of the issue, steps to reproduce, and its potential impact.

Please include as much detail as you can:

- The version/commit you found the issue on
- Whether it requires local access, a malicious character card/import file, a malicious provider response, or something else to trigger
- Any proof-of-concept

## What counts

Janus is a local-first desktop app. Most of its attack surface is: malicious character card imports (embedded PNG/JSON parsing), malicious or compromised provider responses (LLM/image/video API replies), and the Tauri IPC boundary between the frontend and the Rust backend (e.g. path traversal in file operations, SSRF via provider base URLs).

When the admin turns on **browser access**, Janus also runs an HTTP server (off by default, loopback-only unless LAN access is enabled) with user accounts. Reports about it are in scope and welcome: authentication and session handling, the ownership checks that keep accounts' data separate (one account reading or changing another's data), CSRF and cross-origin protection, the file and upload endpoints, and rate limiting and lockout. Browser access serves plain HTTP and is documented as suitable only for a trusted home network; the absence of TLS on its own is a known limitation rather than a vulnerability.

## Response

This is a young, mostly solo-maintained project — there's no formal SLA yet, but security reports get priority over regular issues. Expect an acknowledgment within a few days.

## Disclosure

Please give a reasonable amount of time to address a reported vulnerability before any public disclosure. Credit will be given in the fix's release notes unless you'd prefer to stay anonymous.
