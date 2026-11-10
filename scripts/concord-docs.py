#!/usr/bin/env python3
"""Sync orbital's documents to its Concord project page, and prove each one landed.

    python3 scripts/concord-docs.py            # sync + verify
    python3 scripts/concord-docs.py --dry-run  # print the plan only

Why a script: a project page typed by hand is a second, stale copy of the tree. This one sends the
repo's own files and then re-reads them byte for byte, because a 200 is not the page changing.

The kind is by filename, deliberately — `PUT` upserts on (kind, slug), so sending ROADMAP.md as
`spec` creates a SECOND document beside the real plan instead of updating it.

Auth: `~/.secrets/concord-api-token.txt` (raw `clt_…`, mode 600) plus the consent cookie the write
path requires. Both are needed; neither implies the other.
"""
import argparse
import http.cookiejar
import json
import os
import sys
import urllib.request

BASE = os.environ.get("CONCORD_BASE", "https://concord.polarisocial.xyz/api/v1")
PROJECT = os.environ.get("CONCORD_PROJECT", "orbital")
TOKEN_FILE = os.path.expanduser("~/.secrets/concord-api-token.txt")
UA = "curl/8.6.0"  # Cloudflare answers 1010 to Python's default agent

DOCS = [
    # (relative to the repo root, kind, slug, title)
    ("README.md", "readme", "readme", "README"),
    ("docs/SPEC.md", "spec", "spec", "SPEC"),
    ("docs/ROADMAP.md", "plan", "plan", "ROADMAP (the ledger)"),
    ("docs/DECISIONS.md", "adr", "decisions", "DECISIONS"),
    ("docs/SHARED-RECOMMENDATION.md", "wiki", "shared-recommendation",
     "What orbital shares with kindred's recommender"),
    ("docs/monetization.md", "wiki", "monetization", "Monetization (measured, not claimed)"),
]
VAULT_DOCS = [
    ("~/secondbrain/10-Projects/orbital-ideas-disposition.md", "wiki", "brainstorm-disposition",
     "2026-11-10 brainstorm — disposition"),
    ("~/secondbrain/10-Projects/orbital-fame-headwind-2026-11-10.md", "wiki", "fame-headwind",
     "The fame headwind — 20 ideas, filed and dispositioned (D34)"),
]


class Client:
    def __init__(self):
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))
        self.token = open(TOKEN_FILE).read().strip()

    def call(self, method, path, body=None):
        req = urllib.request.Request(f"{BASE}{path}", method=method)
        req.add_header("User-Agent", UA)
        req.add_header("Authorization", f"Bearer {self.token}")
        data = None
        if body is not None:
            data = json.dumps(body).encode()
            req.add_header("Content-Type", "application/json")
        with self.opener.open(req, data=data) as r:
            raw = r.read()
        return json.loads(raw) if raw else None

    def consent(self):
        self.call("POST", "/consent", {})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    plan = []
    for rel, kind, slug, title in DOCS:
        path = os.path.join(root, rel)
        plan.append((path, kind, slug, title))
    for raw, kind, slug, title in VAULT_DOCS:
        plan.append((os.path.expanduser(raw), kind, slug, title))

    c = Client()
    c.consent()
    existing = {d["slug"] + "|" + d["kind"]: d
                for d in (c.call("GET", f"/projects/{PROJECT}/documents") or [])}
    print(f"{len(existing)} documents on the page now")

    for path, kind, slug, title in plan:
        body = open(path).read()
        key = slug + "|" + kind
        same = existing.get(key) and existing[key]["body"] == body
        state = "unchanged" if same else ("update" if key in existing else "create")
        if args.dry_run:
            print(f"  {state:9s} {kind:9s} {slug:22s} {len(body):7d} B  {os.path.basename(path)}")
            continue
        if same:
            print(f"  unchanged {kind:9s} {slug:22s} {len(body):7d} B")
            continue
        c.call("PUT", f"/projects/{PROJECT}/documents",
               {"kind": kind, "slug": slug, "title": title, "body": body})
        print(f"  {state:9s} {kind:9s} {slug:22s} {len(body):7d} B")

    if args.dry_run:
        return 0
    # Read back through the same API and compare bytes: a 200 is not the page changing.
    after = {d["slug"] + "|" + d["kind"]: d
             for d in (c.call("GET", f"/projects/{PROJECT}/documents") or [])}
    bad = []
    for path, kind, slug, title in plan:
        body = open(path).read()
        got = after.get(slug + "|" + kind)
        if not got:
            bad.append(f"{slug} ({kind}) missing")
        elif got["body"] != body:
            bad.append(f"{slug} ({kind}) differs: sent {len(body)} B, page has {len(got['body'])} B")
    if bad:
        print("FAIL\n  " + "\n  ".join(bad))
        return 1
    print(f"OK {len(plan)} documents match byte for byte; page holds {len(after)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
