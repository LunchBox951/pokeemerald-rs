"""Channel identity and failure-safe publication through the GitHub boundary."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from channel_release import ChannelRelease, publish


class ChannelReleaseTest(unittest.TestCase):
    def test_nightly_and_main_tags_cannot_collide(self):
        nightly = ChannelRelease("unstable", "1.2.3.4")
        main = ChannelRelease("main", "1.2.3.4")
        self.assertEqual(nightly.tag, "v1.2.3.4-nightly")
        self.assertEqual(main.tag, "v1.2.3.4")
        self.assertTrue(nightly.prerelease)
        self.assertFalse(main.prerelease)
        self.assertTrue(ChannelRelease("main", "0.2.3.4").prerelease)

    def test_unapproved_channels_and_malformed_versions_are_rejected(self):
        for channel, version in (("dev", "1.2.3.4"), ("stable", "1.2.3.4"),
                                 ("main", "1.2.3"), ("unstable", "1.2.3.4\nbad")):
            with self.subTest(channel=channel, version=version), self.assertRaises(ValueError):
                ChannelRelease(channel, version)


class PublicationTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.release = ChannelRelease("unstable", "0.0.31.0")
        for archive in self.release.archives(self.root):
            archive.parent.mkdir()
            archive.write_bytes(b"archive")
        self.state = None
        self.upload_fails = False
        self.commands = []
        mocked = patch("channel_release.subprocess.run", side_effect=self.github)
        mocked.start()
        self.addCleanup(mocked.stop)

    def github(self, arguments, **_kwargs):
        args = arguments[1:]
        self.commands.append(args)
        if args[0] == "api":
            return subprocess.CompletedProcess(arguments, 0 if self.state else 1,
                                               json.dumps(self.state), "HTTP 404" if not self.state else "")
        if args[:2] == ["release", "create"]:
            self.assertIn("--draft", args)
            self.assertIn("--verify-tag", args)
            self.assertIn("--prerelease=true", args)
            self.state = {"draft": True, "assets": []}
        elif args[:2] == ["release", "upload"]:
            self.assertTrue(self.state["draft"])
            if self.upload_fails:
                return subprocess.CompletedProcess(arguments, 1, "", "upload interrupted")
            self.state["assets"] = [{"name": Path(path).name, "state": "uploaded", "size": 7}
                                    for path in args[args.index("--clobber") + 1:]]
        elif args[:2] == ["release", "edit"]:
            self.assertEqual(len(self.state["assets"]), 3)
            self.assertIn("--latest=false", args)
            self.state["draft"] = False
        else:
            self.fail(f"unexpected command: {args}")
        return subprocess.CompletedProcess(arguments, 0, "", "")

    def publish(self):
        publish(self.release, "owner/repo", self.root, "a" * 40)

    def test_missing_platform_archive_never_creates_a_release(self):
        self.release.archives(self.root)[1].unlink()
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual(self.commands, [])

    def test_interrupted_upload_stays_private_and_retry_completes_it(self):
        self.upload_fails = True
        with self.assertRaises(RuntimeError):
            self.publish()
        self.assertTrue(self.state["draft"])
        self.upload_fails = False
        self.publish()
        self.assertFalse(self.state["draft"])
        self.assertEqual(sum(args[:2] == ["release", "create"] for args in self.commands), 1)

    def test_published_release_is_never_replaced_on_retry(self):
        self.publish()
        self.commands.clear()
        self.publish()
        self.assertEqual(len(self.commands), 1)
        self.assertEqual(self.commands[0][0], "api")

    def test_incomplete_published_release_is_reported(self):
        self.state = {"draft": False, "assets": []}
        with self.assertRaisesRegex(RuntimeError, "missing required assets"):
            self.publish()


if __name__ == "__main__":
    unittest.main()
