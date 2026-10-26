#!/usr/bin/env python3
"""Backtest the proposed diff-based CI gates on merged PRs.

Usage: backtest.py <data-dir-with-pr_files.jsonl-and-prs.jsonl> [--examples]
"""
import json
import os
import re
import sys

SRC = re.compile(r"^components/.+/src/.+\.rs$")
TESTY = re.compile(r"(/tests?/|_tests?\.rs$|/test_[^/]*\.rs$|/benches/|/examples?/|/fixtures?/|integration-tests)")
WRAPPER = re.compile(r"^components/.+/(android/src/main|ios/).+\.(kt|swift)$")
UDL = re.compile(r"\.udl$")
EXPORT_LINE = re.compile(r"^[+-](?![+-]).*(#\[uniffi::|uniffi::(Record|Enum|Object|Error)|\bpub fn\b.*->|^\s*pub \w+: )")
SCHEMA_LINE = re.compile(r"^[+-](?![+-]).*\b(CREATE\s+(TABLE|INDEX|UNIQUE INDEX|TRIGGER|VIEW|VIRTUAL TABLE)|ALTER\s+TABLE|DROP\s+(TABLE|INDEX))\b", re.I)
VERSION_LINE = re.compile(r"^[+-](?![+-]).*\b(pub(\(crate\))?\s+)?const\s+(DB_|END_)?VERSION\b")
TEST_ADDED = re.compile(r"^\+.*(#\[test\]|#\[tokio::test\]|fn test_)")


def added_removed(patch):
    return [l for l in (patch or "").splitlines() if l[:1] in "+-" and not l.startswith(("+++", "---"))]


def main():
    data = sys.argv[1]
    prs = {}
    with open(os.path.join(data, "prs.jsonl")) as f:
        for line in f:
            p = json.loads(line)
            prs[p["number"]] = p
    rows = [json.loads(l) for l in open(os.path.join(data, "pr_files.jsonl"))]

    stats = {k: {"relevant": 0, "flagged": 0, "examples": []} for k in
             ("changelog-required", "uniffi-api-break-gate", "schema-change-needs-migration", "tests-with-behaviour")}

    def hit(rule, pr, flagged):
        stats[rule]["relevant"] += 1
        if flagged:
            stats[rule]["flagged"] += 1
            if len(stats[rule]["examples"]) < 8:
                stats[rule]["examples"].append(f"#{pr} {prs[pr]['title'][:70]}")

    for row in rows:
        pr, files = row["pr"], row["files"]
        if len(files) >= 100:  # API caps the file list; skip giant PRs (vendoring, release merges)
            continue
        paths = [f["path"] for f in files]
        changelog = [f for f in files if f["path"] == "CHANGELOG.md"]
        changelog_text = "\n".join(added_removed(changelog[0]["patch"])) if changelog else ""
        src = [f for f in files if SRC.match(f["path"]) and not TESTY.search(f["path"])]
        consumer_visible = src + [f for f in files if UDL.search(f["path"]) or WRAPPER.match(f["path"])]

        if consumer_visible:
            hit("changelog-required", pr, not changelog)

        api_lines = [l for f in files if not TESTY.search(f["path"]) and (UDL.search(f["path"]) or SRC.match(f["path"]))
                     for l in added_removed(f["patch"])
                     if UDL.search(f["path"]) or EXPORT_LINE.match(l)]
        removed_api = [l for l in api_lines if l.startswith("-")]
        if removed_api:
            hit("uniffi-api-break-gate", pr, "breaking" not in changelog_text.lower())

        schema_files = [f for f in files if f["path"].startswith("components/") and not TESTY.search(f["path"])
                        and f["status"] == "modified"
                        and any(SCHEMA_LINE.match(l) for l in added_removed(f["patch"]))]
        if schema_files:
            version_bumped = any(VERSION_LINE.match(l) for f in files for l in added_removed(f["patch"]))
            hit("schema-change-needs-migration", pr, not version_bumped)

        src_additions = sum(f["additions"] for f in src)
        if src_additions >= 20:
            tests_touched = any(TESTY.search(p) for p in paths) or any(
                TEST_ADDED.match(l) for f in files for l in added_removed(f["patch"]))
            hit("tests-with-behaviour", pr, not tests_touched)

    total = len(rows)
    print(f"merged PRs analysed: {total}")
    for rule, s in stats.items():
        pct = 100 * s["flagged"] / s["relevant"] if s["relevant"] else 0
        print(f"{rule}: relevant {s['relevant']}, flagged {s['flagged']} ({pct:.0f}% of relevant, "
              f"{100 * s['flagged'] / total:.0f}% of all PRs)")
        if "--examples" in sys.argv:
            for e in s["examples"]:
                print("    ", e)
    json.dump(stats, open(os.path.join(data, "backtest.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
