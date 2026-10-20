"""Fail a pull request that introduces a network destination the code did not already use.

Spyware and data leaks arrive as a new URL or host in the source. For every
line a PR adds under src/ and src-tauri/src/ (tests excluded), find the hosts
in URLs and flag any host that appears nowhere in the base branch. A maintainer
who has reviewed the new destination adds the `network-reviewed` label to pass.
"""
import json
import os
import re
import subprocess
import sys

BASE = os.environ.get("BASE_SHA", "origin/master")
LABELS = {l.get("name") for l in json.loads(os.environ.get("PR_LABELS", "[]"))}
URL = re.compile(r"https?://([A-Za-z0-9][A-Za-z0-9.-]*[A-Za-z0-9])")
IGNORED = {"localhost", "127.0.0.1", "0.0.0.0", "example.com", "www.w3.org", "schema.tauri.app"}
PATHS = ["src", "src-tauri/src"]


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=False).stdout


def is_test(path: str) -> bool:
    return "/tests/" in path or ".test." in path or path.endswith("_test.rs")


def added_hosts() -> dict[str, set[str]]:
    found: dict[str, set[str]] = {}
    path = ""
    for line in git("diff", "-U0", f"{BASE}...HEAD", "--", *PATHS).splitlines():
        if line.startswith("+++ b/"):
            path = line[6:]
        elif line.startswith("+") and not line.startswith("+++") and not is_test(path):
            for host in URL.findall(line):
                found.setdefault(host.lower(), set()).add(path)
    return found


def known_in_base(host: str) -> bool:
    return bool(git("grep", "-i", "-F", "-l", host, BASE, "--", *PATHS).strip())


def main() -> int:
    new = {h: p for h, p in added_hosts().items() if h not in IGNORED and not known_in_base(h)}
    if not new:
        print("No new network destinations.")
        return 0
    print("This change adds network destinations that the code did not use before:\n")
    for host, files in sorted(new.items()):
        print(f"  {host}   ({', '.join(sorted(files))})")
    if "network-reviewed" in LABELS:
        print("\nA maintainer has reviewed them (label `network-reviewed`).")
        return 0
    print(
        "\nJanus promises to talk only to the providers a person adds and to GitHub for updates."
        "\nA maintainer must review each destination and add the `network-reviewed` label."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
