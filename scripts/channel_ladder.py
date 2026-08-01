#!/usr/bin/env python3
"""Pure routing rules for the direct release-channel ladder."""

from __future__ import annotations

import argparse


SOURCE_FOR_TARGET = {
    "unstable": "dev",
    "stable": "unstable",
    "main": "stable",
}

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

    pair_parser = subparsers.add_parser("pair")
    pair_parser.add_argument("target")

    schedule_parser = subparsers.add_parser("operation-for-schedule")
    schedule_parser.add_argument("schedule")

    args = parser.parse_args()
    try:
        if args.command == "source-for":
            print(source_for(args.target))
        elif args.command == "pair":
            print(promotion_pair(args.target))
        else:
            print(operation_for_schedule(args.schedule))
    except ValueError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
