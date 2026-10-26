#!/usr/bin/env python3
"""Print a compact listing of all candidate rules extracted per chunk, grouped
by area, to help cluster near-duplicates. Usage: list_rules.py <out-dir>"""
import collections
import glob
import json
import os
import sys

rows = []
for path in sorted(glob.glob(os.path.join(sys.argv[1], "rules_chunk_*.json"))):
    chunk = json.load(open(path))
    tag = chunk["chunk"].replace("chunk_", "c")
    for r in chunk["rules"]:
        rows.append((r.get("area", "other"), r.get("count", 0), tag, r["id"], r["title"], r.get("enforcement", "")))
for path in sorted(glob.glob(os.path.join(sys.argv[1], "fixes_*.json"))):
    f = json.load(open(path))
    tag = f["file"].replace("fixes_", "f")
    for r in f["rules"]:
        rows.append((r.get("area", "other"), len(r.get("prs", [])), tag, r["id"], r["title"], r.get("enforcement", "")))

by_area = collections.defaultdict(list)
for row in rows:
    by_area[row[0]].append(row)
for area in sorted(by_area, key=lambda a: -sum(r[1] for r in by_area[a])):
    print(f"## {area} ({sum(r[1] for r in by_area[area])})")
    for _, count, tag, rid, title, enf in sorted(by_area[area], key=lambda r: r[3]):
        print(f"  {tag}:{rid} [{count}] {title} <{enf}>")
