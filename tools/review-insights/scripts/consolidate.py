#!/usr/bin/env python3
"""Merge per-chunk candidate rules into the canonical rules from canon.py.

Usage: consolidate.py <out-dir> <threads.jsonl> <result.json>

Checks every candidate maps to exactly one canonical rule, then computes per
rule: review threads, periods seen, bugfix PRs, and evidence links.
"""
import collections
import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from canon import CANON, GROUPS, UNMAPPED_OK  # noqa: E402

REPO = "https://github.com/mozilla/application-services"


def main():
    out_dir, threads_path, result_path = sys.argv[1:4]
    urls = {}
    with open(threads_path) as f:
        for line in f:
            t = json.loads(line)
            urls[(t["kind"], t["id"])] = t["url"]

    candidates, periods, fix_prs = {}, {}, {}
    for path in sorted(glob.glob(os.path.join(out_dir, "rules_chunk_*.json"))):
        chunk = json.load(open(path))
        tag = chunk["chunk"].replace("chunk_", "c")
        periods[tag] = chunk["period"]
        for r in chunk["rules"]:
            candidates[f"{tag}:{r['id']}"] = r
    for path in sorted(glob.glob(os.path.join(out_dir, "fixes_*.json"))):
        f = json.load(open(path))
        tag = f["file"].replace("fixes_", "f")
        for r in f["rules"]:
            candidates[f"{tag}:{r['id']}"] = r
        for p in f["prs"]:
            fix_prs[p["pr"]] = p

    seen = collections.Counter(m for c in CANON for m in c["members"])
    dupes = [m for m, n in seen.items() if n > 1]
    missing = [m for m in seen if m not in candidates]
    unmapped = sorted(set(candidates) - set(seen) - UNMAPPED_OK)
    if dupes or missing:
        sys.exit(f"duplicate members: {dupes}\nunknown members: {missing}")

    rules = []
    for c in CANON:
        threads, chunks, prs, evidence, evidence_prs = 0, set(), set(), [], set()
        for m in c["members"]:
            tag = m.split(":")[0]
            cand = candidates[m]
            if tag.startswith("c"):
                threads += int(cand.get("count", 0))
                chunks.add(tag)
                for e in cand.get("evidence", []):
                    kind, ident = e["ref"].split()[:2]
                    evidence_prs.add(e["ref"].split("PR#")[-1])
                    url = urls.get((kind, int(ident)))
                    if url:
                        evidence.append({"chunk": tag, "url": url, "quote": e["quote"], "pr": e["ref"].split("PR#")[-1]})
            else:
                prs.update(cand.get("prs", []))
        # Spread evidence across periods: round-robin over chunks, newest first.
        by_chunk = collections.defaultdict(list)
        for e in evidence:
            by_chunk[e["chunk"]].append(e)
        picked, order = [], sorted(by_chunk, reverse=True)
        while len(picked) < 4 and any(by_chunk.values()):
            for ch in order:
                if by_chunk[ch] and len(picked) < 4:
                    picked.append(by_chunk[ch].pop(0))
        fixes = []
        for n in sorted(prs):
            p = fix_prs.get(n, {})
            fixes.append({"pr": n, "url": f"{REPO}/pull/{n}", "title": p.get("title", ""), "bug": p.get("bug"),
                          "preventable": p.get("preventable_by_lint")})
        rules.append({
            "id": c["id"], "group": c["group"], "group_title": GROUPS[c["group"]], "tier": c["tier"],
            "title": c["title"], "how": c["how"], "review_threads": threads, "review_prs_min": len(evidence_prs),
            "periods_seen": len(chunks), "bugfix_prs": fixes, "evidence": picked, "members": c["members"],
        })

    fixes_summary = collections.Counter()
    for p in fix_prs.values():
        if p.get("is_bugfix"):
            fixes_summary[p.get("preventable_by_lint", "no")] += 1
    json.dump({
        "periods": periods,
        "candidates": len(candidates),
        "unmapped": unmapped,
        "bugfix_summary": {"candidates": len(fix_prs), "real_bugfixes": sum(fixes_summary.values()), **fixes_summary},
        "bugfix_prs": sorted(fix_prs.values(), key=lambda p: -p["pr"]),
        "rules": rules,
    }, open(result_path, "w"), indent=1, ensure_ascii=False)
    print(f"{len(candidates)} candidates -> {len(rules)} rules; unmapped: {unmapped}", file=sys.stderr)


if __name__ == "__main__":
    main()
