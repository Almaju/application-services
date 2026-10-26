#!/usr/bin/env python3
"""Print a comma-separated list of PR numbers from prs.jsonl, for `fetch.py --files`.

Usage:
  select_prs.py <prs.jsonl> fixes [--since 2022]   likely bugfix PRs (by title keywords)
  select_prs.py <prs.jsonl> merged --since 2025-01-01
"""
import json
import re
import sys

FIX = re.compile(
    r"\b(fix(es|ed|ing)?|crash(es|ing)?|panic(s|king)?|regress(ion)?|broken|incorrect(ly)?|wrong|bug(gy)? in|revert|"
    r"back(ed)? ?out|hotfix|oops|missing|doesn'?t|don'?t|not (being|working)|fail(s|ing|ure)?|deadlock|leak|corrupt|"
    r"overflow|race|hang|ANR|OOM|prevent|avoid|handle|ensure|properly|should)\b", re.I)
NOISE = re.compile(
    r"\b(dependabot|bump|update (to|the)? ?(glean|nss|uniffi|rust|ndk|xcode|gradle)|version|release|nightly|changelog|"
    r"docs?\b|typo|lint|clippy|ci\b|taskcluster|cron|README)", re.I)


def main():
    path, mode = sys.argv[1], sys.argv[2]
    since = sys.argv[sys.argv.index("--since") + 1] if "--since" in sys.argv else "2022"
    prs = [json.loads(line) for line in open(path)]
    date_field = "created_at" if mode == "fixes" else "merged_at"
    picked = [p for p in prs if p["merged_at"] and p[date_field] >= since and "dependabot" not in (p["user"] or "")]
    if mode == "fixes":
        picked = [p for p in picked if FIX.search(p["title"]) and not NOISE.search(p["title"])]
    print(",".join(str(p["number"]) for p in picked))


if __name__ == "__main__":
    main()
