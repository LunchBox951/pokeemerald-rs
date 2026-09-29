#!/usr/bin/env python3
"""Name channel archives and publish complete, retryable GitHub releases."""

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import subprocess


@dataclass(frozen=True)
class ChannelRelease:
    channel: str
    version: str

    def __post_init__(self):
        if self.channel not in ("unstable", "main"):
            raise ValueError("only unstable and main publish releases")
        if re.fullmatch(r"[0-9]+(?:\.[0-9]+){3}", self.version) is None:
            raise ValueError("expected a four-component VERSION")

    @property
    def package(self):
        return self.version + ("-nightly" if self.channel == "unstable" else "")

    @property
    def tag(self):
        return f"v{self.package}"

    @property
    def prerelease(self):
        return self.channel == "unstable" or self.version.split(".")[0] == "0"

    def archives(self, root):
        return [
            root / "linux" / f"pokeemerald-rs-{self.package}-linux.tar.gz",
            root / "windows" / f"pokeemerald-rs-{self.package}-windows.zip",
        ]


def gh(*arguments):
    return subprocess.run(
        ["gh", *arguments], capture_output=True, text=True, timeout=120, check=False
    )


def require_success(result):
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return result.stdout


def publish(release, repository, artifact_root, sha):
    """Keep uploads private until complete; never replace published archives."""
    archives = release.archives(artifact_root)
    for archive in archives:
        if not archive.is_file() or archive.stat().st_size == 0:
            raise ValueError(f"missing or empty release archive: {archive}")
    checksums = artifact_root / "SHA256SUMS"
    checksums.write_text("".join(
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
        for path in archives
    ))
    assets = [*archives, checksums]
    # The CLI also finds drafts; REST's release-by-tag endpoint does not.
    lookup = gh("release", "view", release.tag, "--repo", repository, "--json", "isDraft,assets")
    if lookup.returncode:
        if lookup.stderr.strip() != "release not found":
            require_success(lookup)
        notes = artifact_root / "release-notes.md"
        notes.write_text(
            f"Channel: `{release.channel}`. Version: `{release.version}`. Commit: `{sha}`.\n\n"
            "Download the archive for your operating system and follow its README to "
            "import your own supported Emerald ROM. No game assets are included.\n\n"
            + ("Nightlies pass CI and may contain unknown gameplay or save issues. "
               "Back up saves before experimenting or moving them between channels.\n"
               if release.channel == "unstable" else "")
        )
        require_success(gh(
            "release", "create", release.tag, "--repo", repository, "--draft",
            "--verify-tag", "--title", f"{'Nightly' if release.channel == 'unstable' else 'Release'} {release.version}",
            "--notes-file", str(notes), "--generate-notes",
            f"--prerelease={'true' if release.prerelease else 'false'}",
        ))
    else:
        existing = json.loads(lookup.stdout)
        if not existing["isDraft"]:
            uploaded = {asset["name"] for asset in existing["assets"]
                        if asset.get("state") == "uploaded" and asset.get("size", 0) > 0}
            if not {path.name for path in assets}.issubset(uploaded):
                raise RuntimeError("published release is missing required assets")
            return
    require_success(gh("release", "upload", release.tag, "--repo", repository,
                       "--clobber", *(str(path) for path in assets)))
    flags = ["--latest=false"] if release.channel == "unstable" else []
    require_success(gh("release", "edit", release.tag, "--repo", repository,
                       "--draft=false", *flags))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("metadata", "publish"))
    parser.add_argument("--channel", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--repository")
    parser.add_argument("--sha")
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args()
    release = ChannelRelease(args.channel, args.version)
    if args.operation == "metadata":
        print(f"version={release.version}\ntag={release.tag}\npackage={release.package}")
    else:
        if (not args.repository or not args.artifacts or not args.sha
                or re.fullmatch(r"[0-9a-f]{40}", args.sha) is None):
            parser.error("publish requires --repository, --artifacts, and a full --sha")
        publish(release, args.repository, args.artifacts, args.sha)


if __name__ == "__main__":
    main()
