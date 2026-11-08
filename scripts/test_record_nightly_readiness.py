#!/usr/bin/env python3
"""Tests for the local, owner-authenticated nightly readiness recorder."""

from dataclasses import dataclass, field
import unittest
from unittest.mock import patch

import record_nightly_readiness


SHA = "a" * 40


@dataclass
class FakeClient:
    """Controllable readiness client used to exercise fail-closed behavior."""

    repository: str = "owner/project"
    login: str = "owner"
    live_sha: str = SHA
    conclusions: dict[str, str] = field(
        default_factory=lambda: {
            check: "success" for check in record_nightly_readiness.REQUIRED_CHECKS
        }
    )
    published: list[str] = field(default_factory=list)

    @property
    def owner(self) -> str:
        return self.repository.split("/", maxsplit=1)[0]

    def authenticated_login(self) -> str:
        return self.login

    def live_dev_sha(self) -> str:
        return self.live_sha

    def latest_check_conclusion(self, _sha: str, name: str) -> str:
        return self.conclusions.get(name, "")

    def publish_success(self, sha: str) -> None:
        self.published.append(sha)


class RecordReadinessTest(unittest.TestCase):
    def test_owner_can_publish_exact_green_dev_sha(self):
        client = FakeClient()
        record_nightly_readiness.record_readiness(client, SHA)
        self.assertEqual(client.published, [SHA])

    def test_rejects_non_owner_identity(self):
        client = FakeClient(login="github-actions[bot]")
        with self.assertRaisesRegex(
            record_nightly_readiness.ReadinessError,
            "not repository owner",
        ):
            record_nightly_readiness.record_readiness(client, SHA)
        self.assertEqual(client.published, [])

    def test_rejects_non_full_sha(self):
        client = FakeClient()
        with self.assertRaisesRegex(
            record_nightly_readiness.ReadinessError,
            "full lowercase commit SHA",
        ):
            record_nightly_readiness.record_readiness(client, "abc123")
        self.assertEqual(client.published, [])

    def test_rejects_stale_dev_sha(self):
        client = FakeClient(live_sha="b" * 40)
        with self.assertRaisesRegex(
            record_nightly_readiness.ReadinessError,
            "live dev",
        ):
            record_nightly_readiness.record_readiness(client, SHA)
        self.assertEqual(client.published, [])

    def test_rejects_each_missing_or_failed_check(self):
        for required in record_nightly_readiness.REQUIRED_CHECKS:
            with self.subTest(required=required):
                client = FakeClient(conclusions={
                    check: "success"
                    for check in record_nightly_readiness.REQUIRED_CHECKS
                    if check != required
                })
                with self.assertRaisesRegex(
                    record_nightly_readiness.ReadinessError,
                    required.replace("(", r"\(").replace(")", r"\)"),
                ):
                    record_nightly_readiness.record_readiness(client, SHA)
                self.assertEqual(client.published, [])


class GitHubClientTest(unittest.TestCase):
    @patch("record_nightly_readiness.run_gh")
    def test_latest_check_conclusion_uses_newest_matching_run(self, run_gh):
        run_gh.return_value = """[
          {"check_runs": [
            {"id": 10, "name": "merge-gate / dev", "conclusion": "failure"},
            {"id": 11, "name": "another check", "conclusion": "success"}
          ]},
          {"check_runs": [
            {"id": 12, "name": "merge-gate / dev", "conclusion": "success"}
          ]}
        ]"""
        client = record_nightly_readiness.GitHubClient("owner/project")

        conclusion = client.latest_check_conclusion(SHA, "merge-gate / dev")

        self.assertEqual(conclusion, "success")
        self.assertIn("--paginate", run_gh.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
