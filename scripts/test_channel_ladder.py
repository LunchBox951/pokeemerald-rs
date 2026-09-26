#!/usr/bin/env python3
"""Regression tests for the direct channel ladder and its workflow contract."""

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

import channel_ladder


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
PROMOTE_WORKFLOW = (REPOSITORY_ROOT / ".github/workflows/promote.yml").read_text()
CI_WORKFLOW = (REPOSITORY_ROOT / ".github/workflows/ci.yml").read_text()
RELEASE_WORKFLOW = (REPOSITORY_ROOT / ".github/workflows/release.yml").read_text()
RELEASE_WORKFLOW_FLAT = RELEASE_WORKFLOW.replace("\\\n", " ")
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

    def test_exact_target_for_every_promotion_source(self):
        self.assertEqual(channel_ladder.target_for("dev"), "unstable")
        self.assertEqual(channel_ladder.target_for("unstable"), "stable")
        self.assertEqual(channel_ladder.target_for("stable"), "main")

    def test_unknown_source_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "unknown channel source"):
            channel_ladder.target_for("feature/example")

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


class VersionValidationModeTest(unittest.TestCase):
    REPOSITORY = "LunchBox951/pokeemerald-rs"

    def mode(self, event, **kwargs):
        return channel_ladder.version_validation_mode(event, **kwargs)

    def test_ordinary_pull_requests_are_strict_transitions(self):
        cases = (
            {"base": "dev", "head": "feature/ci-fix"},
            {"base": "stable", "head": "feature/ci-fix"},
            {"base": "stable", "head": "dev"},
            {"base": "unstable", "head": "stable"},
        )
        for refs in cases:
            with self.subTest(**refs):
                self.assertEqual(
                    self.mode(
                        "pull_request",
                        **refs,
                        head_repository=self.REPOSITORY,
                        repository=self.REPOSITORY,
                    ),
                    channel_ladder.TRANSITION_MODE,
                )

    def test_exact_same_repository_promotions_are_cumulative(self):
        for head, base in channel_ladder.TARGET_FOR_SOURCE.items():
            with self.subTest(head=head, base=base):
                self.assertEqual(
                    self.mode(
                        "pull_request",
                        base=base,
                        head=head,
                        head_repository=self.REPOSITORY,
                        repository=self.REPOSITORY,
                    ),
                    channel_ladder.CUMULATIVE_MODE,
                )

    def test_fork_promotion_names_fall_back_to_strict(self):
        for head, base in channel_ladder.TARGET_FOR_SOURCE.items():
            with self.subTest(head=head, base=base):
                self.assertEqual(
                    self.mode(
                        "pull_request",
                        base=base,
                        head=head,
                        head_repository="someone/fork",
                        repository=self.REPOSITORY,
                    ),
                    channel_ladder.TRANSITION_MODE,
                )

    def test_channel_pushes_and_dispatch_are_cumulative(self):
        for ref in (*channel_ladder.TARGET_FOR_SOURCE, "main"):
            with self.subTest(event="push", ref=ref):
                self.assertEqual(
                    self.mode("push", ref=ref),
                    channel_ladder.CUMULATIVE_MODE,
                )
        self.assertEqual(
            self.mode("workflow_dispatch", ref="dev"),
            channel_ladder.CUMULATIVE_MODE,
        )

    def test_unknown_events_and_push_refs_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "unknown channel push ref"):
            self.mode("push", ref="feature/example")
        with self.assertRaisesRegex(ValueError, "unsupported validation event"):
            self.mode("schedule", ref="dev")


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

    def test_nightly_uses_ci_without_owner_recorded_readiness(self):
        self.assertNotIn("status_creator", PROMOTE_WORKFLOW)
        self.assertNotIn("release-readiness", PROMOTE_WORKFLOW)


class NightlyMergeTest(unittest.TestCase):
    SHA = "d" * 40

    def run_merge(self, *, failed_check=None, merge_state="CLEAN", moved=False):
        function = re.search(
            r"( *)merge_unstable\(\) \{.*?\n\1\}", PROMOTE_WORKFLOW, re.DOTALL
        ).group(0)
        checks = [
            {"name": name, "conclusion": "FAILURE" if name == failed_check else "SUCCESS"}
            for name in ("merge-gate / unstable", "source-gate / unstable",
                         "dependency-review", "codeql (actions)",
                         "codeql (python)", "codeql (rust)")
        ]
        pr = {
            "author": {"login": "promoter[bot]"}, "baseRefName": "unstable",
            "baseRefOid": "b" * 40, "headRefName": "dev", "headRefOid": self.SHA,
            "isDraft": False, "mergeStateStatus": merge_state, "statusCheckRollup": checks,
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "pr.json").write_text(json.dumps(pr))
            (root / "gh").write_text("""#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
root = Path(os.environ["GH_FIXTURE"])
if args[:2] == ["pr", "list"]:
    print(1250 if "[0].number" in args[-1] else 1)
elif args[:2] == ["pr", "view"]:
    print((root / "pr.json").read_text())
elif args[:2] == ["api", "graphql"]:
    print(0)
elif args[:3] == ["api", "--method", "PUT"]:
    (root / "merged.json").write_text(json.dumps(args))
    print("true")
else:
    sys.exit("unexpected GitHub request: " + repr(args))
""")
            (root / "gh").chmod(0o755)
            live_head = "e" * 40 if moved else self.SHA
            script = (
                "set -euo pipefail\n"
                'APP_LOGIN="promoter[bot]"\nREPOSITORY="owner/repo"\n'
                'REPOSITORY_OWNER="owner"\n'
                f'branch_sha() {{ if [[ "$1" == dev ]]; then echo {live_head}; '
                f'else echo {"b" * 40}; fi; }}\n'
                + function + "\nmerge_unstable\n"
            )
            result = subprocess.run(["bash", "-c", script], capture_output=True, text=True,
                                    env={**os.environ, "PATH": f"{root}:{os.environ['PATH']}",
                                         "GH_FIXTURE": str(root)})
            self.assertEqual(result.returncode, 0, result.stderr)
            merged = root / "merged.json"
            return json.loads(merged.read_text()) if merged.exists() else None

    def test_green_candidate_merges_without_an_owner_status(self):
        self.assertIn(f"sha={self.SHA}", self.run_merge())

    def test_each_required_ci_failure_keeps_the_previous_nightly(self):
        for check in ("merge-gate / unstable", "source-gate / unstable", "dependency-review",
                      "codeql (actions)", "codeql (python)", "codeql (rust)"):
            with self.subTest(check=check):
                self.assertIsNone(self.run_merge(failed_check=check))

    def test_blocked_rules_or_a_moved_candidate_never_merge(self):
        self.assertIsNone(self.run_merge(merge_state="BLOCKED"))
        self.assertIsNone(self.run_merge(moved=True))


class CiVersionWorkflowContractTest(unittest.TestCase):
    def test_workflow_delegates_mode_selection_to_the_tested_router(self):
        self.assertIn("channel_ladder.py validation-mode", CI_WORKFLOW)
        self.assertEqual(CI_WORKFLOW.count('--mode "${validation_mode}"'), 4)
        self.assertNotIn("expected_target", CI_WORKFLOW)

    def test_event_specific_comparison_refs_remain_wired(self):
        for comparison in (
            '--base "${live_base}" --head HEAD --require-bump',
            '--base "${next_head}" --head HEAD',
            '--base "${push_base}" --head HEAD',
            "--base HEAD --head HEAD",
            # The main-push base is guarded: github.event.before is the
            # zero SHA on branch creation and unreachable after a
            # force-push, so the workflow falls back to HEAD^1 the same
            # way release.yml already does.
            'git cat-file -e "${push_base}^{commit}"',
        ):
            self.assertIn(comparison, CI_WORKFLOW)
        self.assertNotIn("origin/main", CI_WORKFLOW)

    def test_release_requires_a_cumulative_main_push_bump(self):
        self.assertIn("BEFORE_SHA: ${{ github.event.before || '' }}", RELEASE_WORKFLOW)
        self.assertRegex(
            RELEASE_WORKFLOW_FLAT,
            r"version_check\.py --mode cumulative\s+"
            r'--base \"\$\{base\}\" --head HEAD --require-bump',
        )


class SourceGateWorkflowContractTest(unittest.TestCase):
    def test_exact_direct_sources_are_embedded_in_the_base_workflow(self):
        for target, source in channel_ladder.SOURCE_FOR_TARGET.items():
            self.assertRegex(
                SOURCE_GATE_WORKFLOW,
                rf'{re.escape(target)}\)\s+expected_head="{re.escape(source)}"',
            )

    def test_source_gate_rebinds_to_live_pull_request_state(self):
        for live_field in (
            "live_base",
            "live_head",
            "live_head_sha",
            "live_head_repo",
            "live_author",
        ):
            self.assertIn(live_field, SOURCE_GATE_WORKFLOW)
        self.assertIn('if [[ "${duplicates}" != "1" ]]', SOURCE_GATE_WORKFLOW)
        self.assertEqual(SOURCE_GATE_WORKFLOW.count(".headRepository.nameWithOwner"), 2)

    def test_source_gate_requires_app_author_and_preceding_provenance(self):
        self.assertIn(
            "PROMOTION_APP_LOGIN: ${{ vars.PROMOTION_APP_LOGIN }}",
            SOURCE_GATE_WORKFLOW,
        )
        self.assertIn(
            '"${live_author}" != "${PROMOTION_APP_LOGIN}"',
            SOURCE_GATE_WORKFLOW,
        )
        self.assertIn('stable) preceding_head="dev"', SOURCE_GATE_WORKFLOW)
        self.assertIn('main)   preceding_head="unstable"', SOURCE_GATE_WORKFLOW)
        self.assertIn(".mergeCommit.oid ==", SOURCE_GATE_WORKFLOW)
        self.assertIn("${EVENT_HEAD_SHA}", SOURCE_GATE_WORKFLOW)
        self.assertIn('if [[ "${provenance}" != "1" ]]', SOURCE_GATE_WORKFLOW)


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
