#!/usr/bin/env python3
"""Render candidate bugfix PRs (metadata + trimmed patches) into reading chunks.

Usage: render_fixes.py <data-dir> <out-dir> [--chunks 2]
"""
import json, os, re, sys

SKIP = re.compile(r"(Cargo\.lock|DEPENDENCIES\.md|\.lock$|/dumps/|\.json$|\.pbxproj$|CHANGELOG\.md$|/fixtures?/)")
TEST = re.compile(r"(/tests?/|_test\.|test_|Test\.kt|Tests\.swift)")


def trim(text, limit):
    text = re.sub(r"<!--.*?-->", "", text or "", flags=re.S).strip()
    return text if len(text) <= limit else text[:limit] + " […]"


def main():
    data, out = sys.argv[1], sys.argv[2]
    n = int(sys.argv[sys.argv.index("--chunks") + 1]) if "--chunks" in sys.argv else 2
    prs = {}
    with open(os.path.join(data, "prs.jsonl")) as f:
        for line in f:
            p = json.loads(line)
            prs[p["number"]] = p
    rendered = []
    with open(os.path.join(data, "pr_files.jsonl")) as f:
        for line in f:
            row = json.loads(line)
            p = prs[row["pr"]]
            text = f"=== PR#{p['number']} {p['merged_at'][:10]} @{p['user']}: {p['title']}\n"
            text += "  body: " + trim(p["body"], 1200).replace("\n", "\n        ") + "\n"
            files = sorted(row["files"], key=lambda f: (bool(SKIP.search(f["path"])), bool(TEST.search(f["path"]))))
            budget = 3000
            for fl in files:
                text += f"  file {fl['status']} {fl['path']} (+{fl['additions']}/-{fl['deletions']})\n"
                if SKIP.search(fl["path"]) or not fl["patch"] or budget <= 0:
                    continue
                limit = min(budget, 800 if TEST.search(fl["path"]) else 2500)
                patch = trim(fl["patch"], limit)
                budget -= len(patch)
                text += "    | " + patch.replace("\n", "\n    | ") + "\n"
            rendered.append(text)
    size = sum(map(len, rendered)) / n
    chunk, buf = 1, []
    for text in rendered:
        buf.append(text)
        if sum(map(len, buf)) >= size and chunk < n:
            open(os.path.join(out, f"fixes_{chunk}.txt"), "w").write("\n".join(buf))
            chunk, buf = chunk + 1, []
    open(os.path.join(out, f"fixes_{chunk}.txt"), "w").write("\n".join(buf))


if __name__ == "__main__":
    main()
