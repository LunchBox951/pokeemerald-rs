#!/usr/bin/env python3
"""Publish owner-authenticated, SHA-bound nightly release readiness."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import re
import subprocess
import sys
from typing import Any, Sequence


REQUIRED_CHECKS = (
    "merge-gate / dev",
    "codeql (actions)",
    "codeql (python)",
    "codeql (rust)",
)
FULL_SHA = re.compile(r"[0-9a-f]{40}")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")


class ReadinessError(RuntimeError):
    """A failed identity, candidate, or CI readiness requirement."""


def run_gh(arguments: Sequence[str]) -> str:
    """Run GitHub CLI without a shell and return its standard output."""
    result = subprocess.run(
        ["gh", *arguments],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "unknown error"
        raise ReadinessError(f"gh {' '.join(arguments[:2])} failed: {detail}")
    return result.stdout.strip()


@dataclass(frozen=True)
class GitHubClient:
    """Minimal GitHub interface used by the local Birch readiness recorder."""

    repository: str

    @property
    def owner(self) -> str:
        """Return the repository owner whose local identity must publish."""
        return self.repository.split("/", maxsplit=1)[0]

    def authenticated_login(self) -> str:
        """Return the user backing the local GitHub CLI credential."""
        return run_gh(("api", "user", "--jq", ".login"))

    def live_dev_sha(self) -> str:
        """Return the current remote dev tip."""
        return run_gh(
            (
                "api",
                f"repos/{self.repository}/branches/dev",
                "--jq",
                ".commit.sha",
            )
        )

    def latest_check_conclusion(self, sha: str, name: str) -> str:
        """Return the conclusion of the newest check run with the given name."""
        payload = run_gh(
            (
                "api",
                "--paginate",
                "--slurp",
                "-H",
                "Accept: application/vnd.github+json",
                f"repos/{self.repository}/commits/{sha}/check-runs?per_page=100",
            )
        )
        pages: list[dict[str, Any]] = json.loads(payload)
        matches = [
            check
            for page in pages
            for check in page.get("check_runs", [])
            if check.get("name") == name
        ]
        if not matches:
            return ""
        latest = max(matches, key=lambda check: int(check.get("id", 0)))
        return str(latest.get("conclusion") or "")

    def publish_success(self, sha: str) -> None:
        """Publish the readiness status as the authenticated repository owner."""
        run_gh(
            (
                "api",
                "--method",
                "POST",
                f"repos/{self.repository}/statuses/{sha}",
                "-f",
                "state=success",
                "-f",
                "context=release-readiness",
                "-f",
                "description=Birch official-ROM command passed",
                "-f",
                f"target_url=https://github.com/{self.repository}/commit/{sha}",
            )
        )


def record_readiness(client: GitHubClient, candidate_sha: str) -> None:
    """Validate identity, live SHA, and checks before publishing readiness."""
    if FULL_SHA.fullmatch(candidate_sha) is None:
        raise ReadinessError("candidate_sha must be a full lowercase commit SHA")

    login = client.authenticated_login()
    if login != client.owner:
        raise ReadinessError(
            f"authenticated GitHub identity is {login}, not repository owner "
            f"{client.owner}"
        )

    live_dev = client.live_dev_sha()
    if candidate_sha != live_dev:
        raise ReadinessError(
            f"Birch tested {candidate_sha}, but live dev is {live_dev}; "
            "test the current candidate"
        )

    for required in REQUIRED_CHECKS:
        conclusion = client.latest_check_conclusion(candidate_sha, required)
        if conclusion != "success":
            raise ReadinessError(
                f"{required} is {conclusion or 'missing'} on {candidate_sha}"
            )

    client.publish_success(candidate_sha)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    """Parse the tested commit and optional explicit repository."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate_sha", help="full dev SHA tested by Birch")
    parser.add_argument(
        "--repository",
        help="owner/name repository (defaults to the current gh repository)",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    """Run the local readiness publication command."""
    args = parse_args(argv if argv is not None else sys.argv[1:])
    repository = args.repository or run_gh(
        ("repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner")
    )
    if REPOSITORY.fullmatch(repository) is None:
        print(f"error: invalid repository: {repository}", file=sys.stderr)
        return 2

    try:
        record_readiness(GitHubClient(repository), args.candidate_sha)
    except (ReadinessError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    print(f"recorded release-readiness for {repository}@{args.candidate_sha}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
