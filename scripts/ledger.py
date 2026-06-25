#!/usr/bin/env python3
"""
ledger.py — coverage ledger for the pokeemerald-rs rewrite.

The ledger is a JSON file (one per upstream project) tracking which upstream
files and asset directories have been accounted for, where their behavior
now lives in the Rust workspace, and why items without a Rust counterpart
were dropped or folded into another module.

This is a passive bookkeeping tool. It does NOT dispatch agents. Specs in
the v1 acceptance criteria and GitHub issues drive work; this ledger is consulted at planning time and
updated post-merge to keep an audit trail of what got rewritten/ported and
what intentionally didn't.

The ledger file (`ledger/pokeemerald.json`) is committed to the repo
and is the source of truth at every commit. Update only via this CLI so
the canonical JSON format stays diff-friendly.

USAGE
─────

    ledger.py status                   one-line summary per project
    ledger.py categories               counts per category
    ledger.py report                   detailed markdown report
    ledger.py scan                     refresh entry list from disk
    ledger.py gaps [--prefix X | --category K]
                                       list pending entries
    ledger.py inspect <path>           show one entry's row

    ledger.py mark <path> --target <rust-path> --spec <id> --reason "..."
    ledger.py port <path> --target <rust-path> --spec <id> --reason "..."
    ledger.py stub <path> --target <rust-path> --spec <id> --reason "..."
    ledger.py fold <path> --into   <target>    --spec <id> --reason "..."
    ledger.py drop <path>                       --spec <id> --reason "..."
    ledger.py unmark <path>                      reset to pending

    ledger.py verify                   check rust_target paths still exist
    ledger.py audit <spec-id>          suggest mappings from a merge commit
    ledger.py migrate                  upgrade v1 ledger to current schema
    ledger.py init                     create empty ledgers (one-time)

STATUSES
────────

    pending     untouched (default; the only non-terminal status)
    rewritten   code re-implemented in Rust at `rust_target`
    ported      data/asset extracted/transcoded into `rust_target`
    stubbed     typed shell exists at `rust_target`, behavior deferred
    folded      behavior absorbed into `fold_target` (must be specific)
    dropped     intentionally excluded from the Rust port

The `rewritten` vs `ported` distinction is informational: rewritten = code
re-implementation, ported = data/asset extracted via the asset pipeline.
Both are terminal "done" states.

ENTRY SHAPE
───────────

Every entry carries:
    status       one of the statuses above
    category     a category key (see `ledger.py categories`)
    kind         "file" or "dir"

Optional:
    spec_owner   predicted owning spec (informational, only on pending)
    spec         the spec under which work was done (required when terminal)
    reason       why this status was chosen (required when terminal)
    rust_target  where in the workspace it lives (rewritten/ported/stubbed)
    fold_target  what module absorbed it (folded only)

CONSTRAINTS (enforced)
──────────────────────

    - Every entry needs `status`, `category`, `kind`.
    - Every non-pending entry needs `spec` and `reason`.
    - `rewritten`, `ported`, `stubbed` require `rust_target`.
    - `folded` requires `fold_target`. Vague targets (`various`,
      `everywhere`, `scattered`, `tbd`) are rejected — fold honestly or
      drop honestly, never both.
    - `dropped` must not carry `rust_target` or `fold_target`.
    - Pending entries may carry only `status`, `category`, `kind`,
      `spec_owner` (no spec / reason / rust_target / fold_target).
"""

import argparse
import fnmatch
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path, PurePosixPath

REPO_ROOT = Path(__file__).resolve().parent.parent
LEDGER_DIR = REPO_ROOT / "ledger"
SCHEMA_VERSION = 2

TERMINAL_STATUSES = ("rewritten", "ported", "stubbed", "folded", "dropped")
ALL_STATUSES = ("pending",) + TERMINAL_STATUSES
INVALID_FOLD_TARGETS = {"various", "everywhere", "scattered", "tbd", "many", "multiple"}

REF_COMMENT_RE = re.compile(r"//\s*(?:STUB:\s*from\s+|from\s+)pokeemerald/([\w./-]+)")


# ─── category rules ────────────────────────────────────────────────────────
#
# Each Rule produces ledger entries under one category key. Multiple rules
# may share a category key (e.g. trainer front_pics + back_pics).
#
# Modes:
#   "files" — every file matching any pattern
#   "dirs"  — every directory matching any pattern
#
# Patterns are project-root-relative globs interpreted by Path.glob (`**`
# allowed; `*` does not cross `/`). `exclude` patterns use Path.match and
# kill any match they hit.

class Rule:
    __slots__ = ("category", "mode", "patterns", "exclude", "spec_owner")

    def __init__(self, category, mode, patterns, exclude=(), spec_owner=None):
        self.category = category
        self.mode = mode
        self.patterns = tuple(patterns)
        self.exclude = tuple(exclude)
        self.spec_owner = spec_owner


POKEEMERALD_RULES = [
    # ─ CODE ────────────────────────────────────────────────────────────────
    Rule("code.source",    "files", ["src/**/*.c"]),
    Rule("code.header",    "files", ["src/**/*.h", "include/**/*.h"],
         exclude=["include/constants/*.h"]),
    Rule("code.constants", "files", ["include/constants/*.h"]),

    # ─ DATA ────────────────────────────────────────────────────────────────
    Rule("data.map",     "dirs",  ["data/maps/*"],    spec_owner="05-assets"),
    Rule("data.map",     "files", ["data/maps/map_groups.json"],
         spec_owner="05-assets"),
    Rule("data.layout",  "dirs",  ["data/layouts/*"], spec_owner="05-assets"),
    Rule("data.layout",  "files", ["data/layouts/layouts.json"],
         spec_owner="05-assets"),
    Rule("data.tileset", "dirs",  ["data/tilesets/primary/*",
                                   "data/tilesets/secondary/*"],
         spec_owner="05-assets"),
    Rule("data.script",  "files", [
        "data/event_scripts.s",
        "data/field_effect_scripts.s",
        "data/scripts/*.inc",
        "data/specials.inc",
        "data/script_cmd_table.inc",
    ], spec_owner="06-engine"),
    Rule("data.script",  "files", [
        "data/battle_scripts_1.s",
        "data/battle_scripts_2.s",
        "data/battle_anim_scripts.s",
        "data/battle_ai_scripts.s",
        "data/contest_ai_scripts.s",
    ], spec_owner="07-battle"),
    Rule("data.script",  "files", ["data/sound_data.s"],
         spec_owner="03-audio"),
    Rule("data.script",  "files", [
        "data/mystery_event_script_cmd_table.s",
        "data/mystery_gift.s",
        "data/multiboot_berry_glitch_fix.s",
        "data/multiboot_ereader.s",
        "data/multiboot_pokemon_colosseum.s",
        "data/maps.s",
        "data/map_events.s",
    ]),  # no predicted owner: out-of-scope (link/mystery/multiboot) or build glue
    Rule("data.text",    "files", ["data/text/*.inc"], spec_owner="06-engine"),
    Rule("data.misc",    "files", ["data/*.gba"]),  # multiboot blobs

    # ─ GRAPHICS ────────────────────────────────────────────────────────────
    Rule("graphics.pokemon",      "dirs", ["graphics/pokemon/*"],
         spec_owner="05-assets"),
    Rule("graphics.trainer",      "files", ["graphics/trainers/front_pics/*.png",
                                            "graphics/trainers/back_pics/*.png"],
         spec_owner="05-assets"),
    Rule("graphics.object_event", "dirs",  ["graphics/object_events/pics/*"],
         spec_owner="05-assets"),
    Rule("graphics.item",         "files", ["graphics/items/icons/*.png"],
         spec_owner="05-assets"),
    Rule("graphics.font",         "dirs", ["graphics/fonts"],
         spec_owner="02-rendering"),
    Rule("graphics.ui", "dirs", [
        "graphics/bag", "graphics/balls", "graphics/berries",
        "graphics/decorations", "graphics/diploma", "graphics/easy_chat",
        "graphics/frontier_pass", "graphics/interface", "graphics/link",
        "graphics/mail", "graphics/map_popup", "graphics/naming_screen",
        "graphics/party_menu", "graphics/picture_frame", "graphics/pokedex",
        "graphics/pokemon_storage", "graphics/pokenav", "graphics/shop",
        "graphics/starter_choose", "graphics/summary_screen",
        "graphics/text_window", "graphics/trade", "graphics/trainer_card",
        "graphics/types", "graphics/union_room_chat", "graphics/wallclock",
        "graphics/wireless_status_screen", "graphics/wonder_card",
        "graphics/wonder_news",
    ], spec_owner="05-assets"),
    Rule("graphics.battle", "dirs", [
        "graphics/battle_anims", "graphics/battle_environment",
        "graphics/battle_frontier", "graphics/battle_interface",
        "graphics/battle_transitions", "graphics/trainer_hill",
    ], spec_owner="05-assets"),
    Rule("graphics.scene", "dirs", [
        "graphics/berry_blender", "graphics/berry_crush", "graphics/berry_fix",
        "graphics/birch_speech", "graphics/cable_car", "graphics/cave_transition",
        "graphics/contest", "graphics/credits", "graphics/dodrio_berry_picking",
        "graphics/door_anims", "graphics/evolution_scene",
        "graphics/field_effects", "graphics/intro", "graphics/pokeblock",
        "graphics/pokemon_jump", "graphics/rayquaza_scene",
        "graphics/reset_rtc_screen", "graphics/rotating_gates",
        "graphics/roulette", "graphics/slot_machine", "graphics/title_screen",
        "graphics/weather",
    ], spec_owner="05-assets"),
    Rule("graphics.misc", "dirs",  ["graphics/misc", "graphics/unused"],
         spec_owner="05-assets"),
    Rule("graphics.misc", "files", ["graphics/pokemon/unused_garbage.bin"],
         spec_owner="05-assets"),

    # ─ AUDIO ───────────────────────────────────────────────────────────────
    Rule("audio.song",       "files", ["sound/songs/midi/*.mid"],
         spec_owner="03-audio"),
    Rule("audio.voicegroup", "files", ["sound/voicegroups/*.inc"],
         spec_owner="03-audio"),
    Rule("audio.sample",     "files", ["sound/direct_sound_samples/*",
                                       "sound/programmable_wave_samples/*"],
         spec_owner="03-audio"),
    Rule("audio.misc",       "files", ["sound/*.inc", "sound/*.s"],
         spec_owner="03-audio"),

    # ─ META ────────────────────────────────────────────────────────────────
    Rule("meta.charmap", "files", ["charmap.txt"], spec_owner="02-rendering"),
    Rule("meta.build",   "files", [
        "Makefile", "*.mk", "*.ld", "sym_*.txt", "rom.sha1",
        "asmdiff.sh", "asmdiff.ps1", "build_tools.sh",
    ]),
    Rule("meta.asm", "dirs", ["asm", "libagbsyscall"]),
]

CATEGORY_KEYS = sorted({r.category for r in POKEEMERALD_RULES})

PROJECTS = {
    "pokeemerald": {
        "root": REPO_ROOT / "pokeemerald",
        "rules": POKEEMERALD_RULES,
    },
    # mgba is intentionally not tracked — it is reference-only (reference-only).
}


# ─── data access ────────────────────────────────────────────────────────────

def ledger_path(project: str) -> Path:
    return LEDGER_DIR / f"{project}.json"


def _read_raw(project: str) -> dict:
    p = ledger_path(project)
    if not p.exists():
        return {"schema_version": SCHEMA_VERSION, "project": project, "files": {}}
    return json.loads(p.read_text(encoding="utf-8"))


def load_ledger(project: str) -> dict:
    data = _read_raw(project)
    v = data.get("schema_version")
    if v != SCHEMA_VERSION:
        if v == 1:
            sys.exit(f"{ledger_path(project)}: schema_version=1 (tool={SCHEMA_VERSION}).\n"
                     f"Run: python3 scripts/ledger.py migrate")
        sys.exit(f"{ledger_path(project)}: unknown schema_version={v!r}")
    return data


def save_ledger(project: str, data: dict) -> None:
    LEDGER_DIR.mkdir(parents=True, exist_ok=True)
    data["schema_version"] = SCHEMA_VERSION
    for path, entry in data["files"].items():
        err = validate_entry(entry)
        if err:
            sys.exit(f"ledger validation failed for {path}: {err}")
    text = json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False)
    ledger_path(project).write_text(text + "\n", encoding="utf-8")


def validate_entry(entry: dict):
    status = entry.get("status", "pending")
    if status not in ALL_STATUSES:
        return f"invalid status: {status!r} (valid: {ALL_STATUSES})"

    cat = entry.get("category")
    if not cat:
        return "missing 'category'"
    if cat not in CATEGORY_KEYS:
        return f"unknown category: {cat!r} (valid: {CATEGORY_KEYS})"
    kind = entry.get("kind")
    if kind not in ("file", "dir"):
        return f"invalid kind: {kind!r} (must be 'file' or 'dir')"

    if status == "pending":
        allowed = {"status", "category", "kind", "spec_owner"}
        extras = set(entry.keys()) - allowed
        if extras:
            return f"pending entries must not carry: {sorted(extras)}"
        return None

    for key in ("spec", "reason"):
        if not entry.get(key):
            return f"{status} entries require '{key}'"

    if status in ("rewritten", "ported", "stubbed"):
        if not entry.get("rust_target"):
            return f"{status} requires 'rust_target'"
        if entry.get("fold_target"):
            return f"{status} must not carry 'fold_target'"
    if status == "folded":
        ft = entry.get("fold_target", "")
        if not ft:
            return "folded requires 'fold_target'"
        if ft.lower() in INVALID_FOLD_TARGETS:
            return (f"fold_target must be a single concrete target "
                    f"(got: {ft!r}). If the behavior truly spreads, "
                    f"mark `dropped` and explain in --reason instead.")
        if entry.get("rust_target"):
            return "folded must not carry 'rust_target'"
    if status == "dropped":
        if entry.get("rust_target") or entry.get("fold_target"):
            return "dropped must not carry 'rust_target' or 'fold_target'"
    return None


# ─── filesystem scan ────────────────────────────────────────────────────────

def _excluded(rel: str, patterns) -> bool:
    p = PurePosixPath(rel)
    return any(p.match(ex) for ex in patterns)


def scan_project(project: str):
    """Return {relative_path: (category, kind, spec_owner_or_None)}."""
    cfg = PROJECTS[project]
    root = cfg["root"]
    if not root.exists():
        sys.exit(f"upstream root missing: {root}\nDid you run ./init.sh?")
    found = {}
    for rule in cfg["rules"]:
        kind = "file" if rule.mode == "files" else "dir"
        want_file = (rule.mode == "files")
        for pattern in rule.patterns:
            for p in root.glob(pattern):
                if want_file and not p.is_file():
                    continue
                if not want_file and not p.is_dir():
                    continue
                rel = p.relative_to(root).as_posix()
                if _excluded(rel, rule.exclude):
                    continue
                if rel in found and found[rel][0] != rule.category:
                    sys.exit(f"category conflict for {rel}: "
                             f"{found[rel][0]!r} and {rule.category!r}")
                found[rel] = (rule.category, kind, rule.spec_owner)
    return found


# ─── commands ───────────────────────────────────────────────────────────────

def cmd_scan(args):
    project = args.project
    ledger = load_ledger(project)
    existing = ledger["files"]
    scanned = scan_project(project)
    disk = set(scanned.keys())
    existing_set = set(existing.keys())

    added = sorted(disk - existing_set)
    removed = sorted(existing_set - disk)
    refreshed = 0
    for p in added:
        cat, kind, owner = scanned[p]
        entry = {"status": "pending", "category": cat, "kind": kind}
        if owner:
            entry["spec_owner"] = owner
        existing[p] = entry
    # Re-apply rule-derived fields to existing pending entries — these are
    # owned by the scan rules, not by users, so a rule edit should flow
    # through. Terminal entries keep whatever they were marked with.
    for p in existing_set & disk:
        entry = existing[p]
        if entry.get("status", "pending") != "pending":
            continue
        cat, kind, owner = scanned[p]
        new = {"status": "pending", "category": cat, "kind": kind}
        if owner:
            new["spec_owner"] = owner
        if new != entry:
            existing[p] = new
            refreshed += 1
    pruned = []
    if args.prune:
        for p in removed:
            entry = existing.get(p, {})
            if entry.get("status", "pending") == "pending":
                del existing[p]
                pruned.append(p)

    save_ledger(project, ledger)
    print(f"{project}: {len(disk)} on disk · {len(added)} added · "
          f"{refreshed} refreshed · {len(removed)} no longer on disk · "
          f"{len(pruned)} pruned")
    if removed and not args.prune:
        print("  (use --prune to drop pending entries no longer on disk;")
        print("   terminal entries are kept and reported stale by `verify`)")


def cmd_status(args):
    for project in args.projects:
        ledger = load_ledger(project)
        counts = Counter(e.get("status", "pending") for e in ledger["files"].values())
        total = sum(counts.values())
        terminal = sum(counts[s] for s in TERMINAL_STATUSES)
        pct = (100.0 * terminal / total) if total else 0.0
        parts = [f"[{project}]", f"total={total}"]
        for s in TERMINAL_STATUSES + ("pending",):
            parts.append(f"{s}={counts.get(s, 0)}")
        parts.append(f"coverage={pct:.1f}%")
        print("  ".join(parts))


def cmd_categories(args):
    project = args.project
    ledger = load_ledger(project)
    by_cat = {}
    for path, e in ledger["files"].items():
        cat = e.get("category", "?")
        st = e.get("status", "pending")
        by_cat.setdefault(cat, Counter())[st] += 1
    if not by_cat:
        print(f"{project}: ledger is empty (run `init` or `scan`)")
        return

    width = max(len(c) for c in by_cat)
    header = f"  {'category':<{width}}  total   pending  terminal"
    print(header)
    print("  " + "─" * (len(header) - 2))
    for cat in sorted(by_cat):
        counts = by_cat[cat]
        total = sum(counts.values())
        pending = counts.get("pending", 0)
        terminal = total - pending
        print(f"  {cat:<{width}}  {total:5d}    {pending:5d}     {terminal:5d}")


def cmd_report(args):
    project = args.project
    ledger = load_ledger(project)
    files = ledger["files"]
    counts = Counter(e.get("status", "pending") for e in files.values())
    total = sum(counts.values())
    terminal = sum(counts[s] for s in TERMINAL_STATUSES)
    def pct(n): return (100.0 * n / total) if total else 0.0

    out = []
    out.append(f"# Coverage report — `{project}`")
    out.append("")
    out.append(f"**{terminal}/{total} accounted for** ({pct(terminal):.1f}%)")
    out.append("")
    out.append("| Status | Count | % |")
    out.append("|---|---:|---:|")
    for s in TERMINAL_STATUSES + ("pending",):
        c = counts.get(s, 0)
        out.append(f"| {s} | {c} | {pct(c):.1f}% |")
    out.append(f"| **total** | **{total}** | 100% |")

    # Per-category breakdown
    by_cat = {}
    for e in files.values():
        cat = e.get("category", "?")
        st = e.get("status", "pending")
        by_cat.setdefault(cat, Counter())[st] += 1
    if by_cat:
        out.append("")
        out.append("## By category")
        out.append("")
        out.append("| Category | Total | Pending | Terminal | Coverage |")
        out.append("|---|---:|---:|---:|---:|")
        for cat in sorted(by_cat):
            c = by_cat[cat]
            t = sum(c.values())
            p = c.get("pending", 0)
            te = t - p
            cov = (100.0 * te / t) if t else 0.0
            out.append(f"| `{cat}` | {t} | {p} | {te} | {cov:.1f}% |")

    # By spec (actual)
    spec_counts = Counter()
    for e in files.values():
        if e.get("status") in TERMINAL_STATUSES and e.get("spec"):
            spec_counts[e["spec"]] += 1
    if spec_counts:
        out.append("")
        out.append("## Accounted for, by spec")
        out.append("")
        out.append("| Spec | Items |")
        out.append("|---|---:|")
        for spec, c in sorted(spec_counts.items()):
            out.append(f"| `{spec}` | {c} |")

    # By predicted owner (informational, pending only)
    owner_counts = Counter()
    for e in files.values():
        if e.get("status", "pending") == "pending" and e.get("spec_owner"):
            owner_counts[e["spec_owner"]] += 1
    if owner_counts:
        out.append("")
        out.append("## Pending, by predicted spec_owner")
        out.append("")
        out.append("| Predicted owner | Pending |")
        out.append("|---|---:|")
        for owner, c in sorted(owner_counts.items()):
            out.append(f"| `{owner}` | {c} |")

    print("\n".join(out))


def cmd_inspect(args):
    ledger = load_ledger(args.project)
    if args.path not in ledger["files"]:
        sys.exit(f"not in ledger: {args.project}:{args.path}\n"
                 f"(maybe the entry was added since the last `scan`?)")
    entry = ledger["files"][args.path]
    print(f"# {args.project}:{args.path}\n")
    for k in ("status", "category", "kind", "spec_owner",
              "rust_target", "fold_target", "spec", "reason"):
        if k in entry:
            print(f"{k+':':14s} {entry[k]}")


def cmd_gaps(args):
    ledger = load_ledger(args.project)
    pending = []
    for path, e in ledger["files"].items():
        if e.get("status", "pending") != "pending":
            continue
        if args.prefix and not path.startswith(args.prefix):
            continue
        if args.category and e.get("category") != args.category:
            continue
        pending.append(path)
    pending.sort()
    n = len(pending)
    limit = args.limit if args.limit > 0 else n
    for p in pending[:limit]:
        print(p)
    if n > limit:
        print(f"\n... +{n - limit} more (use --limit 0 to show all)", file=sys.stderr)
    print(f"{n} pending entr{'y' if n == 1 else 'ies'}", file=sys.stderr)


def _set_entry(project, path, status, *, rust_target=None, fold_target=None,
               spec=None, reason=None):
    ledger = load_ledger(project)
    if path not in ledger["files"]:
        sys.exit(f"unknown entry: {project}:{path}\n"
                 f"Run `ledger.py scan` first, or check the path.")
    cur = ledger["files"][path]
    # Preserve category & kind (immutable identity fields); drop spec_owner
    # since it only applies to pending entries.
    new = {
        "status": status,
        "category": cur["category"],
        "kind": cur["kind"],
    }
    if spec is not None:    new["spec"] = spec
    if reason is not None:  new["reason"] = reason
    if rust_target is not None: new["rust_target"] = rust_target
    if fold_target is not None: new["fold_target"] = fold_target
    ledger["files"][path] = new
    save_ledger(project, ledger)
    print(f"{project}:{path} → {status}")


def cmd_mark(args):
    _set_entry(args.project, args.path, "rewritten",
               rust_target=args.target, spec=args.spec, reason=args.reason)


def cmd_port(args):
    _set_entry(args.project, args.path, "ported",
               rust_target=args.target, spec=args.spec, reason=args.reason)


def cmd_stub(args):
    _set_entry(args.project, args.path, "stubbed",
               rust_target=args.target, spec=args.spec, reason=args.reason)


def cmd_fold(args):
    _set_entry(args.project, args.path, "folded",
               fold_target=args.into, spec=args.spec, reason=args.reason)


def cmd_drop(args):
    _set_entry(args.project, args.path, "dropped",
               spec=args.spec, reason=args.reason)


def cmd_unmark(args):
    ledger = load_ledger(args.project)
    if args.path not in ledger["files"]:
        sys.exit(f"unknown entry: {args.project}:{args.path}")
    cur = ledger["files"][args.path]
    entry = {"status": "pending", "category": cur["category"], "kind": cur["kind"]}
    # Re-attach spec_owner if the current rule predicts one.
    scanned = scan_project(args.project)
    if args.path in scanned:
        _, _, owner = scanned[args.path]
        if owner:
            entry["spec_owner"] = owner
    ledger["files"][args.path] = entry
    save_ledger(args.project, ledger)
    print(f"{args.project}:{args.path} → pending")


def cmd_verify(args):
    project = args.project
    ledger = load_ledger(project)
    stale = []
    for path, entry in sorted(ledger["files"].items()):
        target = entry.get("rust_target")
        if not target:
            continue
        if not (REPO_ROOT / target).exists():
            stale.append((path, target))
    if not stale:
        print(f"{project}: all rust_target pointers resolve.")
        return
    print(f"{project}: {len(stale)} stale pointer(s):")
    for path, target in stale:
        print(f"  {path}  rust_target={target}  (missing)")
    sys.exit(1)


def git(*args):
    res = subprocess.run(
        ["git", "-C", str(REPO_ROOT), *args],
        capture_output=True, text=True,
    )
    if res.returncode != 0:
        sys.exit(f"git {' '.join(args)} failed:\n{res.stderr}")
    return res.stdout


# Mapping from Rust filename stems to plausible upstream stems (heuristic).
STEM_HINTS = {
    "rng": ("random",),
    "audio_engine": ("sound", "m4a"),
    "framebuffer": ("bg",),
}


def cmd_audit(args):
    spec = args.spec_id
    base = args.base
    rev = args.rev

    diff = git("diff", "--name-only", f"{base}...{rev}").splitlines()
    rust = [f for f in diff if f.endswith(".rs") and (REPO_ROOT / f).is_file()]

    print(f"# Audit — spec `{spec}` ({base}…{rev})")
    print()
    print(f"**Rust files changed:** {len(rust)}")
    print()
    if not rust:
        print("(none — nothing to audit)")
        return

    ledger = load_ledger(args.project)
    upstream = set(ledger["files"].keys())
    by_stem = {}
    for u in upstream:
        by_stem.setdefault(Path(u).stem, []).append(u)

    suggestions = []  # (upstream, rust_file, current_status, detected_via)
    for rs in rust:
        full = REPO_ROOT / rs
        try:
            text = full.read_text(encoding="utf-8", errors="replace")
        except OSError:
            text = ""
        for m in REF_COMMENT_RE.finditer(text):
            u = m.group(1)
            if u in upstream:
                cur = ledger["files"][u].get("status", "pending")
                suggestions.append((u, rs, cur, "comment marker"))
        rs_stem = Path(rs).stem
        candidate_stems = (rs_stem,) + STEM_HINTS.get(rs_stem, ())
        for stem in candidate_stems:
            for u in by_stem.get(stem, []):
                cur = ledger["files"][u].get("status", "pending")
                suggestions.append((u, rs, cur, "filename"))

    seen, uniq = set(), []
    for s in suggestions:
        key = (s[0], s[1])
        if key in seen:
            continue
        seen.add(key)
        uniq.append(s)

    print("## Rust files changed\n")
    for rs in rust:
        print(f"- `{rs}`")
    print()

    if not uniq:
        print("## Heuristic mappings\n")
        print("None detected. Inspect each Rust file by hand and call "
              "`ledger.py mark/port/fold/drop` per relevant upstream entry.")
        return

    print("## Suggested mappings\n")
    print("| Upstream | Rust target | Current status | Detected via |")
    print("|---|---|---|---|")
    for u, rs, cur, src in uniq:
        print(f"| `{u}` | `{rs}` | `{cur}` | {src} |")
    print()

    pending = [s for s in uniq if s[2] == "pending"]
    if pending:
        print("## Suggested commands\n")
        print("Edit each `--reason` before running. Do not bulk-run blindly.\n")
        print("```bash")
        for u, rs, _, _ in pending:
            print(f'python3 scripts/ledger.py mark {u} \\')
            print(f'    --target {rs} --spec {spec} \\')
            print(f'    --reason "<FILL IN — what behavior moved, anything non-obvious>"')
        print("```")


def cmd_migrate(args):
    del args
    for project in PROJECTS:
        p = ledger_path(project)
        if not p.exists():
            print(f"{p.relative_to(REPO_ROOT)}: missing; skipping")
            continue
        data = _read_raw(project)
        v = data.get("schema_version")
        if v == SCHEMA_VERSION:
            print(f"{p.relative_to(REPO_ROOT)}: already at v{SCHEMA_VERSION}")
            continue
        if v != 1:
            sys.exit(f"{p}: unsupported schema_version={v!r}; manual migration required")

        scanned = scan_project(project)
        old_files = data.get("files", {})
        new_files = {}
        unmatched = []

        for path, entry in old_files.items():
            if path in scanned:
                cat, kind, owner = scanned[path]
            elif path.startswith("include/constants/"):
                cat, kind, owner = "code.constants", "file", None
            elif path.endswith(".h"):
                cat, kind, owner = "code.header", "file", None
            elif path.endswith(".c"):
                cat, kind, owner = "code.source", "file", None
            else:
                unmatched.append(path)
                continue
            entry["category"] = cat
            entry["kind"] = kind
            if entry.get("status", "pending") == "pending" and owner:
                entry["spec_owner"] = owner
            new_files[path] = entry

        if unmatched:
            print(f"WARNING: {len(unmatched)} entries had no rule and no fallback; kept verbatim:")
            for u in unmatched[:10]:
                print(f"  {u}")
            if len(unmatched) > 10:
                print(f"  ... +{len(unmatched) - 10} more")
            for path in unmatched:
                new_files[path] = old_files[path]

        data["schema_version"] = SCHEMA_VERSION
        data["files"] = new_files
        save_ledger(project, data)
        print(f"{p.relative_to(REPO_ROOT)}: migrated v{v} → v{SCHEMA_VERSION} "
              f"({len(new_files)} entries)")


def cmd_init(args):
    del args
    LEDGER_DIR.mkdir(parents=True, exist_ok=True)
    for project in PROJECTS:
        p = ledger_path(project)
        if p.exists():
            print(f"{p.relative_to(REPO_ROOT)}: already exists, leaving alone")
            continue
        save_ledger(project, {"schema_version": SCHEMA_VERSION,
                              "project": project, "files": {}})
        print(f"created {p.relative_to(REPO_ROOT)}")

    class _ScanArgs:
        prune = False
    scan_args = _ScanArgs()
    for project in PROJECTS:
        scan_args.project = project
        cmd_scan(scan_args)


# ─── arg parsing ────────────────────────────────────────────────────────────

def _add_project(p, multi=False):
    if multi:
        p.add_argument("--projects", nargs="+",
                       default=list(PROJECTS), choices=list(PROJECTS))
    else:
        p.add_argument("--project", default="pokeemerald",
                       choices=list(PROJECTS))


def main():
    ap = argparse.ArgumentParser(
        prog="ledger.py",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("scan", help="Refresh entry list from disk")
    _add_project(p)
    p.add_argument("--prune", action="store_true",
                   help="Drop pending entries no longer on disk")

    p = sub.add_parser("status", help="Per-project counts (one line each)")
    _add_project(p, multi=True)

    p = sub.add_parser("categories", help="Per-category counts")
    _add_project(p)

    p = sub.add_parser("report", help="Detailed markdown coverage report")
    _add_project(p)

    p = sub.add_parser("inspect", help="Show one entry's ledger row")
    _add_project(p)
    p.add_argument("path")

    p = sub.add_parser("gaps", help="List pending entries")
    _add_project(p)
    p.add_argument("--prefix", default="", help="Path-prefix filter")
    p.add_argument("--category", default="", help="Category-key filter",
                   choices=[""] + CATEGORY_KEYS)
    p.add_argument("--limit", type=int, default=50,
                   help="Max rows to print (0 = unlimited)")

    for name, status in (("mark", "rewritten"),
                         ("port", "ported"),
                         ("stub", "stubbed")):
        p = sub.add_parser(name, help=f"Mark entry as {status}")
        _add_project(p)
        p.add_argument("path")
        p.add_argument("--target", required=True,
                       help="Rust file or asset path (e.g. crates/engine/src/rng.rs "
                            "or crates/assets/data/maps/littleroot_town)")
        p.add_argument("--spec", required=True, help="Spec id (e.g. 06-engine)")
        p.add_argument("--reason", required=True)

    p = sub.add_parser("fold", help="Mark folded into another module")
    _add_project(p)
    p.add_argument("path")
    p.add_argument("--into", required=True,
                   help="One concrete target (e.g. crates/engine::save). "
                        "Vague targets are rejected.")
    p.add_argument("--spec", required=True)
    p.add_argument("--reason", required=True)

    p = sub.add_parser("drop", help="Mark intentionally excluded")
    _add_project(p)
    p.add_argument("path")
    p.add_argument("--spec", required=True)
    p.add_argument("--reason", required=True)

    p = sub.add_parser("unmark", help="Reset entry to pending")
    _add_project(p)
    p.add_argument("path")

    p = sub.add_parser("verify", help="Check all rust_target paths exist")
    _add_project(p)

    p = sub.add_parser("audit", help="Suggest mappings from a merge commit")
    _add_project(p)
    p.add_argument("spec_id", help="Spec id (e.g. 06-engine)")
    p.add_argument("--base", default="main", help="git base ref (default: main)")
    p.add_argument("--rev", default="HEAD", help="git target ref (default: HEAD)")

    sub.add_parser("migrate", help="Upgrade a v1 ledger to the current schema")
    sub.add_parser("init", help="Create empty ledgers and run first scan")

    args = ap.parse_args()
    handlers = {
        "scan": cmd_scan, "status": cmd_status,
        "categories": cmd_categories, "report": cmd_report,
        "inspect": cmd_inspect, "gaps": cmd_gaps,
        "mark": cmd_mark, "port": cmd_port, "stub": cmd_stub,
        "fold": cmd_fold, "drop": cmd_drop, "unmark": cmd_unmark,
        "verify": cmd_verify, "audit": cmd_audit,
        "migrate": cmd_migrate, "init": cmd_init,
    }
    handlers[args.cmd](args)


if __name__ == "__main__":
    main()
