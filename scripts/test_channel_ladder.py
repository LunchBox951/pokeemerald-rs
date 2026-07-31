#!/usr/bin/env python3
"""Regression tests for the direct channel ladder and its workflow contract."""

from pathlib import Path
import re
import unittest

import channel_ladder


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
PROMOTE_WORKFLOW = (REPOSITORY_ROOT / ".github/workflows/promote.yml").read_text()
SOURCE_GATE_WORKFLOW = (
    REPOSITORY_ROOT / ".github/workflows/channel-merge-policy.yml"
).read_text()
READINESS_RECORDER = (
    REPOSITORY_ROOT / "scripts/record_nightly_readiness.py"
).read_text()


class ChannelLadderTest(unittest.TestCase):
    def test_exact_source_for_every_protected_channel(self):
        self.assertEqual(channel_ladder.source_for("unstable"), "dev")
        self.assertEqual(channel_ladder.source_for("stable"), "unstable")
        self.assertEqual(channel_ladder.source_for("main"), "stable")

    def test_unknown_target_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "unknown channel target"):
            channel_ladder.source_for("release/0.1")

    def test_staggered_toronto_schedule(self):
        self.assertEqual(
            channel_ladder.operation_for_schedule("17 0 * * *"),
            "open-unstable",
        )
        self.assertEqual(
            channel_ladder.operation_for_schedule("47 0 * * *"),
            "open-stable",
        )
        self.assertEqual(
            channel_ladder.operation_for_schedule("17 1 * * *"),
            "open-main",
        )
        self.assertEqual(
            channel_ladder.operation_for_schedule("17 2 * * *"),
            "merge-unstable",
        )

    def test_unknown_schedule_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "unknown promotion schedule"):
            channel_ladder.operation_for_schedule("0 0 * * *")

    def test_only_unstable_has_an_automated_merge_operation(self):
        merge_operations = {
            operation
            for operation in channel_ladder.SCHEDULED_OPERATION.values()
            if operation.startswith("merge-")
        }
        self.assertEqual(merge_operations, {"merge-unstable"})


class PromotionWorkflowContractTest(unittest.TestCase):
    def test_schedule_is_timezone_aware_and_avoids_birch_hours(self):
        self.assertEqual(PROMOTE_WORKFLOW.count("timezone: America/Toronto"), 4)
        for schedule in channel_ladder.SCHEDULED_OPERATION:
            self.assertIn(f'cron: "{schedule}"', PROMOTE_WORKFLOW)

    def test_dedicated_app_has_only_required_explicit_permissions(self):
        for permission in (
            "permission-checks: read",
            "permission-contents: write",
            "permission-pull-requests: write",
            "permission-statuses: read",
        ):
            self.assertIn(permission, PROMOTE_WORKFLOW)
        self.assertNotIn("permission-administration:", PROMOTE_WORKFLOW)
        self.assertNotIn("permission-workflows:", PROMOTE_WORKFLOW)
        self.assertIn("persist-credentials: false", PROMOTE_WORKFLOW)

    def test_only_unstable_calls_the_merge_function(self):
        self.assertIn("merge-unstable) merge_unstable ;;", PROMOTE_WORKFLOW)
        self.assertNotIn("merge_stable", PROMOTE_WORKFLOW)
        self.assertNotIn("merge_main", PROMOTE_WORKFLOW)
        self.assertNotIn("--auto", PROMOTE_WORKFLOW)
        self.assertNotIn("--admin", PROMOTE_WORKFLOW)

    def test_promotion_pr_lookups_exclude_same_named_fork_branches(self):
        self.assertEqual(PROMOTE_WORKFLOW.count(".headRepository.nameWithOwner"), 3)

    def test_existing_promotion_must_have_been_opened_by_the_app(self):
        self.assertIn("--json author,headRepository,number", PROMOTE_WORKFLOW)
        self.assertIn('"${existing_author}" != "${APP_LOGIN}"', PROMOTE_WORKFLOW)
        self.assertIn("refusing to adopt it", PROMOTE_WORKFLOW)

    def test_merge_is_immediate_and_bound_to_the_evaluated_sha(self):
        self.assertIn('"repos/${REPOSITORY}/pulls/${pr_number}/merge"', PROMOTE_WORKFLOW)
        self.assertIn('-f sha="${head_sha}"', PROMOTE_WORKFLOW)
        self.assertIn("-f merge_method=merge", PROMOTE_WORKFLOW)

    def test_player_channels_are_marked_for_manual_owner_review(self):
        self.assertIn('"${target}" == "stable"', PROMOTE_WORKFLOW)
        self.assertIn('"${target}" == "main"', PROMOTE_WORKFLOW)
        self.assertIn("labels+=(--label needs-review --label needs-operator)", PROMOTE_WORKFLOW)
        self.assertIn("never auto-merges", PROMOTE_WORKFLOW)

    def test_unstable_requires_sha_bound_readiness_twice(self):
        self.assertEqual(
            PROMOTE_WORKFLOW.count('status_creator "${source_sha}" release-readiness')
            + PROMOTE_WORKFLOW.count('status_creator "${head_sha}" release-readiness'),
            2,
        )
        self.assertEqual(
            PROMOTE_WORKFLOW.count('"${creator}" != "${REPOSITORY_OWNER}"'),
            2,
        )
        self.assertIn("sort_by(.created_at) | last", PROMOTE_WORKFLOW)
        self.assertNotIn('"github-actions[bot]"', PROMOTE_WORKFLOW)


class SourceGateWorkflowContractTest(unittest.TestCase):
    def test_exact_direct_sources_are_embedded_in_the_base_workflow(self):
        for target, source in channel_ladder.SOURCE_FOR_TARGET.items():
            self.assertRegex(
                SOURCE_GATE_WORKFLOW,
                rf'{re.escape(target)}\)\s+expected_head="{re.escape(source)}"',
            )

    def test_source_gate_rebinds_to_live_pull_request_state(self):
        for live_field in ("live_base", "live_head", "live_head_sha", "live_head_repo"):
            self.assertIn(live_field, SOURCE_GATE_WORKFLOW)
        self.assertIn('if [[ "${duplicates}" != "1" ]]', SOURCE_GATE_WORKFLOW)
        self.assertEqual(SOURCE_GATE_WORKFLOW.count(".headRepository.nameWithOwner"), 1)


class ReadinessRecorderContractTest(unittest.TestCase):
    def test_readiness_is_local_and_owner_authenticated(self):
        self.assertIn("authenticated_login", READINESS_RECORDER)
        self.assertIn("login != client.owner", READINESS_RECORDER)
        self.assertNotIn("github.token", READINESS_RECORDER)
        self.assertFalse(
            (REPOSITORY_ROOT / ".github/workflows/record-nightly-readiness.yml").exists()
        )

    def test_readiness_is_full_sha_bound_and_ci_gated(self):
        self.assertIn('FULL_SHA = re.compile(r"[0-9a-f]{40}")', READINESS_RECORDER)
        self.assertIn("candidate_sha != live_dev", READINESS_RECORDER)
        for required in (
            "merge-gate / dev",
            "codeql (actions)",
            "codeql (python)",
            "codeql (rust)",
        ):
            self.assertIn(required, READINESS_RECORDER)
        self.assertIn('f"repos/{self.repository}/statuses/{sha}"', READINESS_RECORDER)

    def test_readiness_never_accepts_or_uploads_a_rom(self):
        self.assertNotIn("rom_path", READINESS_RECORDER.lower())
        self.assertNotIn("upload-artifact", READINESS_RECORDER)
        self.assertNotIn("actions/checkout", READINESS_RECORDER)


if __name__ == "__main__":
    unittest.main()
