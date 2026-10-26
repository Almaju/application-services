#!/usr/bin/env python3
"""Turn the raw JSONL dumps into reviewer threads, keyword-tag them, and write
LLM-sized chunks.

Usage: prepare.py <data-dir> <out-dir> [--since 2022-01-01] [--chunks 6]

Writes:
  threads.jsonl          one row per reviewer thread / review summary, with tags
  tag_stats.md           tag counts per year (rough, keyword based)
  chunk_N.txt            compact rendering of threads since --since, for reading
"""

import collections
import json
import os
import re
import sys

BOTS = re.compile(r"(\[bot\]|codecov|mergify|dependabot|taskcluster|github-actions)", re.I)
ACK = re.compile(
    r"^\s*(thanks?( you)?( so much)?|done|fixed|good (catch|call|idea|point)|\+1|lgtm|nice|"
    r"agreed?|yes|yep|ok(ay)?|sure|will do|updated|oops|ah|great|r\+|:\+1:|👍)[.!: ]*\W*$",
    re.I,
)

# Rough keyword buckets; a thread can land in several. Matched on the comment
# body, plus the file path where noted.
TAGS = {
    "uniffi": r"uniffi|\budl\b|\.udl|ffi|bindgen|binding|callback interface|megazord|\bRecord\b|\bObject\b.*\bArc|Arc<Self>|#\[uniffi|export",
    "error-handling": r"unwrap|expect\(|panic|\berror|Result<|thiserror|anyhow|\?` |error_support|ApiResult|handle_error|internal error|fallible",
    "logging-privacy": r"\blog(s|ging|ged)?\b|\b(info|warn|error|debug|trace)!|\bpii\b|privacy|sensitive|redact|leak",
    "naming": r"renam|naming|\bname\b|call(ed|ing) it|misleading",
    "docs": r"doc(s|umentation|string| comment)|comment|explain|///|readme",
    "changelog": r"changelog",
    "tests": r"\btest|assert|mock|fixture",
    "sql-db": r"\bsql|sqlite|schema|migrat|transaction|\bindex\b|query|rusqlite|pragma|\bdb\b|database",
    "concurrency": r"\block|mutex|thread|deadlock|race|rwlock|\basync|await|blocking|main thread|atomic",
    "deps-build": r"cargo\.toml|dependenc|feature flag|\bcrate|cargo|vendor|gecko|mozilla-central|\bm-c\b|gradle|build",
    "mobile-bindings": r"kotlin|swift|android|\bios\b|desktop|\bjs\b|javascript|consumer",
    "performance": r"\bclone|alloc|perf|o\(n|\.collect|iterator|copy|expensive|cheap|cache",
    "api-design": r"public api|breaking|backward|compat|\bpub\b|visibility|pub\(crate\)",
    "serde-json": r"serde|json|deserial|serializ",
    "time": r"timestamp|systemtime|instant|millis|seconds|clock|now\(\)|duration|timezone",
    "strings-urls": r"\burl\b|url::|to_string|format!|string",
    "telemetry": r"glean|metrics\.yaml|telemetry|metric",
    "nimbus-fml": r"\bfml\b|feature manifest|\.fml\.yaml",
    "style-nit": r"^\s*nit|clippy|rustfmt|style|\bnit\b|formatting",
    "unsafe": r"\bunsafe\b",
    "magic-values": r"magic (number|string|value)|constant|hard-?coded",
    "simplify": r"simplif|could just|you can just|unnecessary|redundant|not needed|no need",
    "dead-code": r"dead code|unused|not used|leftover|remove this|todo|fixme|commented.out",
}
TAG_RE = {k: re.compile(v, re.I) for k, v in TAGS.items()}


def load(path):
    with open(path) as f:
        return [json.loads(line) for line in f]


def tags_for(text):
    return [k for k, r in TAG_RE.items() if r.search(text)]


def trim(text, limit):
    text = re.sub(r"<!--.*?-->", "", text or "", flags=re.S).strip()
    return text if len(text) <= limit else text[:limit] + " […]"


def hunk_tail(hunk, lines=6):
    return "\n".join((hunk or "").splitlines()[-lines:])


def main():
    data, out = sys.argv[1], sys.argv[2]
    args = sys.argv[3:]
    since = args[args.index("--since") + 1] if "--since" in args else "2022-01-01"
    n_chunks = int(args[args.index("--chunks") + 1]) if "--chunks" in args else 6
    os.makedirs(out, exist_ok=True)

    prs = {p["number"]: p for p in load(os.path.join(data, "prs.jsonl"))}
    rc = load(os.path.join(data, "review_comments.jsonl"))
    rv = load(os.path.join(data, "reviews.jsonl"))

    replies = collections.defaultdict(list)
    for c in rc:
        if c["in_reply_to_id"]:
            replies[c["in_reply_to_id"]].append(c)

    threads = []
    for c in rc:
        pr = prs.get(c["pr"], {})
        if c["in_reply_to_id"] or BOTS.search(c["user"] or "") or ACK.match(c["body"] or ""):
            continue
        if c["user"] == pr.get("user"):  # author annotating their own PR
            continue
        thread_replies = sorted(replies[c["id"]], key=lambda r: r["created_at"])
        threads.append({
            "kind": "inline", "id": c["id"], "pr": c["pr"], "pr_title": pr.get("title", ""),
            "reviewer": c["user"], "created_at": c["created_at"], "path": c["path"],
            "line": c["line"], "body": c["body"], "diff_tail": hunk_tail(c["diff_hunk"]),
            "replies": [{"user": r["user"], "body": r["body"]} for r in thread_replies],
            "url": c["url"], "tags": tags_for((c["body"] or "") + " " + (c["path"] or "")),
        })
    for r in rv:
        pr = prs.get(r["pr"], {})
        if not r["created_at"] or BOTS.search(r["user"] or "") or len(r["body"].strip()) < 80:
            continue
        threads.append({
            "kind": "review", "id": r["id"], "pr": r["pr"], "pr_title": pr.get("title", ""),
            "reviewer": r["user"], "created_at": r["created_at"], "state": r["state"],
            "body": r["body"], "url": r["url"], "tags": tags_for(r["body"]),
        })
    threads.sort(key=lambda t: t["created_at"])

    with open(os.path.join(out, "threads.jsonl"), "w") as f:
        for t in threads:
            f.write(json.dumps(t) + "\n")

    # Tag stats per year.
    years = sorted({t["created_at"][:4] for t in threads})
    counts = {k: collections.Counter() for k in TAGS}
    totals = collections.Counter(t["created_at"][:4] for t in threads)
    for t in threads:
        for k in t["tags"]:
            counts[k][t["created_at"][:4]] += 1
    with open(os.path.join(out, "tag_stats.md"), "w") as f:
        f.write("| tag | total | " + " | ".join(years) + " |\n")
        f.write("|---|---:|" + "---:|" * len(years) + "\n")
        for k in sorted(TAGS, key=lambda k: -sum(counts[k].values())):
            f.write(f"| {k} | {sum(counts[k].values())} | " + " | ".join(
                f"{100 * counts[k][y] / totals[y]:.0f}%" for y in years) + " |\n")
        f.write("| (threads) | " + str(len(threads)) + " | " + " | ".join(str(totals[y]) for y in years) + " |\n")

    # Compact rendering for reading, split into chunks of similar size.
    recent = [t for t in threads if t["created_at"] >= since]
    rendered = []
    for t in recent:
        ref = f"[{t['kind']} {t['id']} PR#{t['pr']} {t['created_at'][:10]} @{t['reviewer']}]"
        if t["kind"] == "inline":
            text = f"{ref} {t['path']}:{t['line']}  (PR: {trim(t['pr_title'], 100)})\n"
            if t["diff_tail"]:
                text += "  code> " + "\n  code> ".join(trim(t["diff_tail"], 600).splitlines()) + "\n"
            text += "  REVIEW: " + trim(t["body"], 1000).replace("\n", "\n          ") + "\n"
            for r in t["replies"][:3]:
                text += f"  reply @{r['user']}: " + trim(r["body"], 300).replace("\n", " ") + "\n"
        else:
            text = f"{ref} review summary ({t['state']})  (PR: {trim(t['pr_title'], 100)})\n"
            text += "  REVIEW: " + trim(t["body"], 1500).replace("\n", "\n          ") + "\n"
        rendered.append(text)
    total = sum(len(r) for r in rendered)
    size, chunk, buf = total / n_chunks, 1, []
    for text in rendered:
        buf.append(text)
        if sum(len(b) for b in buf) >= size and chunk < n_chunks:
            with open(os.path.join(out, f"chunk_{chunk}.txt"), "w") as f:
                f.write("\n".join(buf))
            chunk, buf = chunk + 1, []
    with open(os.path.join(out, f"chunk_{chunk}.txt"), "w") as f:
        f.write("\n".join(buf))
    print(f"threads={len(threads)} recent={len(recent)} chars={total}", file=sys.stderr)


if __name__ == "__main__":
    main()
