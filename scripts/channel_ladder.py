#!/usr/bin/env python3
"""Pure routing rules for the direct release-channel ladder."""

from __future__ import annotations

import argparse


SOURCE_FOR_TARGET = {
    "unstable": "dev",
    "stable": "unstable",
    "main": "stable",
}

TARGET_FOR_SOURCE = {source: target for target, source in SOURCE_FOR_TARGET.items()}

TRANSITION_MODE = "transition"
CUMULATIVE_MODE = "cumulative"

SCHEDULED_OPERATION = {
    "17 0 * * *": "open-unstable",
    "47 0 * * *": "open-stable",
    "17 1 * * *": "open-main",
    "17 2 * * *": "merge-unstable",
}


def source_for(target: str) -> str:
    """Return the only branch allowed to promote into ``target``."""

    try:
        return SOURCE_FOR_TARGET[target]
    except KeyError as error:
        raise ValueError(f"unknown channel target: {target}") from error


def target_for(source: str) -> str:
    """Return the only branch that ``source`` may promote into."""

    try:
        return TARGET_FOR_SOURCE[source]
    except KeyError as error:
        raise ValueError(f"unknown channel source: {source}") from error


def version_validation_mode(
    event: str,
    *,
    ref: str = "",
    base: str = "",
    head: str = "",
    head_repository: str = "",
    repository: str = "",
) -> str:
    """Select strict transition or cumulative endpoint validation for CI."""

    if event == "pull_request":
        same_repository = bool(repository) and head_repository == repository
        direct_pair = TARGET_FOR_SOURCE.get(head) == base
        if same_repository and direct_pair:
            return CUMULATIVE_MODE
        return TRANSITION_MODE

    if event == "push":
        if ref not in (*TARGET_FOR_SOURCE, "main"):
            raise ValueError(f"unknown channel push ref: {ref}")
        return CUMULATIVE_MODE

    if event == "workflow_dispatch":
        return CUMULATIVE_MODE

    raise ValueError(f"unsupported validation event: {event}")


def operation_for_schedule(schedule: str) -> str:
    """Translate a GitHub schedule expression into its promotion operation."""

    try:
        return SCHEDULED_OPERATION[schedule]
    except KeyError as error:
        raise ValueError(f"unknown promotion schedule: {schedule}") from error


def promotion_pair(target: str) -> str:
    """Return the human-readable source-to-target pair."""

    return f"{source_for(target)}->{target}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    source_parser = subparsers.add_parser("source-for")
    source_parser.add_argument("target")

    target_parser = subparsers.add_parser("target-for")
    target_parser.add_argument("source")

    mode_parser = subparsers.add_parser("validation-mode")
    mode_parser.add_argument("--event", required=True)
    mode_parser.add_argument("--ref", default="")
    mode_parser.add_argument("--base", default="")
    mode_parser.add_argument("--head", default="")
    mode_parser.add_argument("--head-repository", default="")
    mode_parser.add_argument("--repository", default="")

    pair_parser = subparsers.add_parser("pair")
    pair_parser.add_argument("target")

    schedule_parser = subparsers.add_parser("operation-for-schedule")
    schedule_parser.add_argument("schedule")

    args = parser.parse_args()
    try:
        if args.command == "source-for":
            print(source_for(args.target))
        elif args.command == "target-for":
            print(target_for(args.source))
        elif args.command == "validation-mode":
            print(
                version_validation_mode(
                    args.event,
                    ref=args.ref,
                    base=args.base,
                    head=args.head,
                    head_repository=args.head_repository,
                    repository=args.repository,
                )
            )
        elif args.command == "pair":
            print(promotion_pair(args.target))
        else:
            print(operation_for_schedule(args.schedule))
    except ValueError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
