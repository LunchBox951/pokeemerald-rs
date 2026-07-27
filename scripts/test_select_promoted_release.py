#!/usr/bin/env python3
"""Regression test for scripts/select_promoted_release.sh (issue #135).

Run with:  python3 -m unittest scripts/test_select_promoted_release.py
       or:  python3 scripts/test_select_promoted_release.py

Builds a throwaway bare "remote" git repository and a working clone to
reproduce the exact scenario from issue #135: two long-lived `release/*`
branches are both ancestors of a channel, and the *older* one (by commit
date) is the one a given push actually promoted. The old committer-date scan
in .github/workflows/promote.yml picked the branch with the newest tip
regardless of which one the triggering merge named; this pins the fixed
behavior of scripts/select_promoted_release.sh, which the workflow now calls.
"""

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

_SCRIPT = Path(__file__).resolve().parent / "select_promoted_release.sh"

_ENV = {
    "GIT_AUTHOR_NAME": "Test",
    "GIT_AUTHOR_EMAIL": "test@example.invalid",
    "GIT_COMMITTER_NAME": "Test",
    "GIT_COMMITTER_EMAIL": "test@example.invalid",
    "GIT_MERGE_AUTOEDIT": "no",
}


def run(cwd, *args, env=None, check=True):
    merged_env = dict(_ENV)
    if env:
        merged_env.update(env)
    result = subprocess.run(
        args,
        cwd=str(cwd),
        env=merged_env,
        capture_output=True,
        text=True,
    )
    if check and result.returncode != 0:
        raise AssertionError(
            f"command failed: {args}\nstdout={result.stdout}\nstderr={result.stderr}"
        )
    return result


def commit(cwd, message, date, filename="file.txt", content="x"):
    (cwd / filename).write_text(content, encoding="utf-8")
    run(cwd, "git", "add", filename)
    run(
        cwd,
        "git",
        "commit",
        "-m",
        message,
        env={"GIT_AUTHOR_DATE": date, "GIT_COMMITTER_DATE": date},
    )
    return run(cwd, "git", "rev-parse", "HEAD").stdout.strip()


def merge(cwd, branch, date, message=None):
    """Merge `branch` with git's default recorded subject (the identity the
    selector reads), or an explicit `message` to simulate a rewritten one."""
    args = ["git", "merge", "--no-ff", branch]
    if message is not None:
        args += ["-m", message]
    run(
        cwd,
        *args,
        env={"GIT_AUTHOR_DATE": date, "GIT_COMMITTER_DATE": date},
    )
    return run(cwd, "git", "rev-parse", "HEAD").stdout.strip()


class SelectPromotedReleaseTest(unittest.TestCase):
    """Two concurrent release/* lines; the unrelated one has the newer tip."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        root = Path(self._tmp.name)
        self.remote = root / "remote.git"
        self.work = root / "work"
        self.checkout = root / "checkout"

        run(root, "git", "init", "--bare", str(self.remote))
        run(root, "git", "clone", str(self.remote), str(self.work))
        run(self.work, "git", "checkout", "-b", "dev")

        base = commit(self.work, "base", "2026-07-01T00:00:00")

        # release/0.2: freshly cut, older tip commit date (this is the one
        # the merges below actually promote).
        run(self.work, "git", "branch", "release/0.2", base)
        run(self.work, "git", "checkout", "release/0.2")
        self.r02_tip = commit(
            self.work,
            "release/0.2 fix",
            "2026-07-10T00:00:00",
            filename="release-0.2.txt",
            content="r0.2",
        )

        # release/0.1: long-lived maintenance line, unrelated to this
        # promotion, but with a NEWER tip commit date. Touches a different
        # file so merging both into unstable below cannot conflict.
        run(self.work, "git", "branch", "release/0.1", base)
        run(self.work, "git", "checkout", "release/0.1")
        self.r01_tip = commit(
            self.work,
            "release/0.1 maintenance fix",
            "2026-07-20T00:00:00",
            filename="release-0.1.txt",
            content="r0.1",
        )

        # unstable: merge release/0.1 first (so it's an ancestor too), then
        # release/0.2 -- the push under test is the release/0.2 merge.
        run(self.work, "git", "checkout", "-b", "unstable", base)
        self.m1 = merge(self.work, "release/0.1", "2026-07-21T00:00:00")
        self.m2 = merge(self.work, "release/0.2", "2026-07-22T00:00:00")

        run(
            self.work,
            "git",
            "push",
            "origin",
            "release/0.1",
            "release/0.2",
            "unstable",
        )

        run(root, "git", "clone", str(self.remote), str(self.checkout))

    def tearDown(self):
        self._tmp.cleanup()

    def select(self, sha, expect_rc=0):
        result = run(self.checkout, str(_SCRIPT), sha, check=False)
        self.assertEqual(
            result.returncode, expect_rc,
            msg=f"stdout={result.stdout!r} stderr={result.stderr!r}",
        )
        return result.stdout.strip()

    def refetch(self):
        run(self.checkout, "git", "fetch", "origin")

    def test_selects_the_branch_the_merge_actually_promoted(self):
        # The triggering push is the release/0.2 merge (m2). Even though
        # release/0.1 has a newer tip commit date and is also an ancestor of
        # unstable, the merge that fired this run promoted release/0.2.
        self.assertEqual(self.select(self.m2), "release/0.2")

    def test_selects_the_other_line_for_its_own_merge(self):
        # Symmetric check: the release/0.1 merge (m1) itself must resolve to
        # release/0.1, not whichever branch happens to sort first.
        self.assertEqual(self.select(self.m1), "release/0.1")

    def _advance_r02(self):
        run(self.work, "git", "checkout", "release/0.2")
        commit(
            self.work,
            "release/0.2 follow-up",
            "2026-07-24T00:00:00",
            filename="release-0.2-followup.txt",
            content="r0.2b",
        )
        run(self.work, "git", "push", "origin", "release/0.2")

    def test_consolidated_line_still_resolves_to_the_promoted_branch(self):
        # Consolidation (RELEASE.md §Consolidating) merges release/0.2 into
        # release/0.1, so release/0.2's tip becomes an ancestor of BOTH
        # lines. The promotion merge of release/0.2 must still resolve to
        # release/0.2 (its recorded identity), not whichever name sorts first.
        run(self.work, "git", "checkout", "release/0.1")
        merge(self.work, "release/0.2", "2026-07-23T00:00:00")
        run(self.work, "git", "push", "origin", "release/0.1")
        self.refetch()
        self.assertEqual(self.select(self.m2), "release/0.2")

    def test_advanced_branch_signals_reclear(self):
        # If the promoted branch gained commits after the merge, its new tip
        # has NOT cleared the current rung: the script must still name the
        # branch but exit 4 so the caller re-clears instead of advancing an
        # unvetted tip.
        self._advance_r02()
        self.refetch()
        self.assertEqual(self.select(self.m2, expect_rc=4), "release/0.2")

    def test_decoy_at_old_tip_of_advanced_branch(self):
        # The confirmed round-2 repro: release/0.2 advances past the merged
        # commit while a sibling ref still sits exactly on it. Current-ref
        # topology alone would pick the stationary decoy; the recorded merge
        # subject must win — and still signal re-clear (exit 4).
        run(self.work, "git", "branch", "release/0.3", self.r02_tip)
        run(self.work, "git", "push", "origin", "release/0.3")
        self._advance_r02()
        self.refetch()
        self.assertEqual(self.select(self.m2, expect_rc=4), "release/0.2")

    def test_same_tip_decoy_resolves_via_recorded_subject(self):
        # Two refs on the same commit are indistinguishable by topology, but
        # the merge subject recorded which one was promoted.
        run(self.work, "git", "branch", "release/0.3", self.r02_tip)
        run(self.work, "git", "push", "origin", "release/0.3")
        self.refetch()
        self.assertEqual(self.select(self.m2), "release/0.2")

    def test_pr_merge_subject_resolves(self):
        # GitHub PR merges record "Merge pull request #N from owner/branch".
        run(self.work, "git", "checkout", "release/0.2")
        commit(
            self.work,
            "release/0.2 second fix",
            "2026-07-25T00:00:00",
            filename="release-0.2-second.txt",
            content="r0.2c",
        )
        run(self.work, "git", "checkout", "unstable")
        m3 = merge(
            self.work,
            "release/0.2",
            "2026-07-25T01:00:00",
            message="Merge pull request #99 from LunchBox951/release/0.2",
        )
        run(self.work, "git", "push", "origin", "release/0.2", "unstable")
        self.refetch()
        self.assertEqual(self.select(m3), "release/0.2")

    def test_unparseable_subject_falls_back_to_topology(self):
        # A rewritten subject loses the recorded identity; an unambiguous
        # topology (one candidate, tip on the merged commit) still resolves.
        run(self.work, "git", "checkout", "release/0.2")
        commit(
            self.work,
            "release/0.2 third fix",
            "2026-07-25T02:00:00",
            filename="release-0.2-third.txt",
            content="r0.2d",
        )
        run(self.work, "git", "checkout", "unstable")
        m3 = merge(
            self.work,
            "release/0.2",
            "2026-07-25T03:00:00",
            message="promote the current release line",
        )
        run(self.work, "git", "push", "origin", "release/0.2", "unstable")
        self.refetch()
        self.assertEqual(self.select(m3), "release/0.2")

    def test_unparseable_subject_with_same_tip_decoy_fails_loudly(self):
        # No recorded identity AND two refs claim the merged commit: the
        # script must refuse (exit 3) rather than pick by name order.
        run(self.work, "git", "checkout", "release/0.2")
        tip = commit(
            self.work,
            "release/0.2 fourth fix",
            "2026-07-25T04:00:00",
            filename="release-0.2-fourth.txt",
            content="r0.2e",
        )
        run(self.work, "git", "checkout", "unstable")
        m3 = merge(
            self.work,
            "release/0.2",
            "2026-07-25T05:00:00",
            message="promote the current release line",
        )
        run(self.work, "git", "branch", "release/0.4", tip)
        run(self.work, "git", "push", "origin", "release/0.2", "release/0.4", "unstable")
        self.refetch()
        result = run(self.checkout, str(_SCRIPT), m3, check=False)
        self.assertEqual(result.returncode, 3, msg=result.stdout + result.stderr)
        self.assertIn("ambiguous", result.stderr)

    def test_non_merge_push_selects_nothing(self):
        # A single-parent commit (no release/* merged in) is a no-op: e.g. a
        # direct commit made on the channel itself.
        run(self.checkout, "git", "checkout", "unstable")
        sha = commit(
            self.checkout,
            "direct channel commit",
            "2026-07-23T00:00:00",
            filename="direct.txt",
            content="direct",
        )
        self.assertEqual(self.select(sha), "")


if __name__ == "__main__":
    unittest.main()
