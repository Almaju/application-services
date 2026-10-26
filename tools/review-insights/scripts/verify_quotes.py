#!/usr/bin/env python3
"""Check that every evidence quote in the extracted candidates appears in the
review comment it cites (ignoring punctuation, case and whitespace).

Usage: verify_quotes.py <threads.jsonl from prepare.py> <candidates-dir>
"""
import collections
import glob
import json
import os
import re
import sys


def norm(text):
    return re.sub(r"[\W_]+", "", text or "").lower()


def main():
    threads_path, cand_dir = sys.argv[1:3]
    bodies = {}
    with open(threads_path) as f:
        for line in f:
            t = json.loads(line)
            bodies[(t["kind"], t["id"])] = norm(t["body"])
    stats, misses = collections.Counter(), []
    for path in sorted(glob.glob(os.path.join(cand_dir, "rules_chunk_*.json"))):
        chunk = json.load(open(path))
        for item in chunk["rules"] + chunk["non_lintable_themes"]:
            for e in item.get("evidence", []):
                kind, ident = e["ref"].split()[:2]
                body = bodies.get((kind, int(ident)), "")
                # Quotes may elide text with "..." / "…"; every fragment must match.
                frags = [norm(f)[:50] for f in re.split(r"\.\.\.|…", e["quote"]) if norm(f)]
                ok = bool(frags) and all(f in body for f in frags)
                stats[ok] += 1
                if not ok:
                    misses.append(f"{e['ref']}: {e['quote'][:80]!r}")
    print(f"matched {stats[True]} of {stats[True] + stats[False]} quotes")
    for m in misses:
        print("  no match:", m)


if __name__ == "__main__":
    main()
