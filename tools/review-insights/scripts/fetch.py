#!/usr/bin/env python3
"""Download PR review data for a GitHub repo as JSON lines, using only REST.

Usage: fetch.py <out-dir> [--skip-base] [--reviews] [--files PR,PR,...]

Writes:
  review_comments.jsonl  inline comments attached to a diff line
  issue_comments.jsonl   PR conversation comments
  prs.jsonl              PR metadata (title, body, labels, merge state)
  reviews.jsonl          review summaries with a non-empty body (--reviews)
  pr_files.jsonl         changed files + patches for the given PRs (--files)
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

REPO = os.environ.get("REPO", "mozilla/application-services")
API = "https://api.github.com"


def get(path):
    req = urllib.request.Request(
        f"{API}/{path}",
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {os.environ.get('GH_TOKEN', '')}",
        },
    )
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                return json.load(resp)
        except urllib.error.HTTPError as e:
            if e.code in (403, 429, 502, 503) and attempt < 4:
                time.sleep(2**attempt * 2)
                continue
            raise
        except OSError:  # URLError, timeouts, connection resets from the proxy
            if attempt < 4:
                time.sleep(2**attempt * 2)
                continue
            raise


def all_pages(path, threads=8):
    """Fetch page=1..N in parallel batches until a page comes back short."""
    sep = "&" if "?" in path else "?"
    out, page, done = [], 1, False
    with ThreadPoolExecutor(threads) as pool:
        while not done:
            pages = range(page, page + threads)
            for items in pool.map(lambda p: get(f"{path}{sep}per_page=100&page={p}"), pages):
                out.extend(items)
                if len(items) < 100:
                    done = True
            page += threads
    return out


def pr_number(url):
    return int(url.rstrip("/").split("/")[-1])


def write(out_dir, name, rows):
    with open(os.path.join(out_dir, name), "w") as f:
        for row in rows:
            f.write(json.dumps(row) + "\n")
    print(f"{name}: {len(rows)}", file=sys.stderr)


def user(item):
    u = item.get("user") or {}
    return {"user": u.get("login"), "user_type": u.get("type"), "assoc": item.get("author_association")}


def fetch_base(out_dir):
    write(out_dir, "review_comments.jsonl", [
        {
            "id": c["id"], "pr": pr_number(c["pull_request_url"]), **user(c),
            "created_at": c["created_at"], "path": c.get("path"),
            "line": c.get("line") or c.get("original_line"),
            "in_reply_to_id": c.get("in_reply_to_id"),
            "diff_hunk": c.get("diff_hunk"), "body": c.get("body"), "url": c["html_url"],
        }
        for c in all_pages(f"repos/{REPO}/pulls/comments?sort=created&direction=desc")
    ])

    write(out_dir, "issue_comments.jsonl", [
        {
            "id": c["id"], "pr": pr_number(c["issue_url"]), **user(c),
            "created_at": c["created_at"], "body": c.get("body"), "url": c["html_url"],
        }
        for c in all_pages(f"repos/{REPO}/issues/comments?sort=created&direction=desc")
        if "/pull/" in c["html_url"]
    ])

    prs = [
        {
            "number": p["number"], "title": p["title"], "body": p.get("body"), **user(p),
            "created_at": p["created_at"], "merged_at": p.get("merged_at"), "state": p["state"],
            "labels": [l["name"] for l in p.get("labels", [])],
        }
        for p in all_pages(f"repos/{REPO}/pulls?state=all&sort=created&direction=desc")
    ]
    write(out_dir, "prs.jsonl", prs)


def main():
    out_dir = sys.argv[1]
    os.makedirs(out_dir, exist_ok=True)
    args = sys.argv[2:]
    if "--skip-base" not in args:
        fetch_base(out_dir)
    with open(os.path.join(out_dir, "prs.jsonl")) as f:
        prs = [json.loads(line) for line in f]

    if "--reviews" in args:
        def reviews(n):
            return [
                {
                    "id": r["id"], "pr": n, **user(r), "state": r["state"],
                    "created_at": r.get("submitted_at"), "body": r["body"], "url": r["html_url"],
                }
                for r in get(f"repos/{REPO}/pulls/{n}/reviews?per_page=100")
                if (r.get("body") or "").strip()
            ]

        with ThreadPoolExecutor(16) as pool:
            rows = [r for rs in pool.map(reviews, [p["number"] for p in prs]) for r in rs]
        write(out_dir, "reviews.jsonl", rows)

    if "--files" in args:
        numbers = [int(n) for n in args[args.index("--files") + 1].split(",")]

        def files(n):
            return {
                "pr": n,
                "files": [
                    {"path": f["filename"], "status": f["status"], "additions": f["additions"],
                     "deletions": f["deletions"], "patch": f.get("patch")}
                    for f in get(f"repos/{REPO}/pulls/{n}/files?per_page=100")
                ],
            }

        with ThreadPoolExecutor(16) as pool:
            write(out_dir, "pr_files.jsonl", list(pool.map(files, numbers)))


if __name__ == "__main__":
    main()
