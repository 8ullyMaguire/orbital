#!/usr/bin/env python3
"""Populate the two owner accounts on a live Orbital.

Idempotent: links already present (same URL) are left alone, and the profile
patch is a set-operation, so re-running it repairs rather than duplicates.

    ORBITAL_BASE=https://links.polarisocial.xyz ./scripts/populate-accounts.py

Credentials are read from ~/.hermes/.env (ORBITAL_CRITICALNEXUS_PASSWORD /
ORBITAL_STEELTITAN_PASSWORD) and never printed.
"""
from __future__ import annotations

import json
import os
import pathlib
import sys
import urllib.error
import urllib.request

BASE = os.environ.get("ORBITAL_BASE", "https://links.polarisocial.xyz").rstrip("/")
ENV = pathlib.Path.home() / ".hermes" / ".env"
UA = "Mozilla/5.0 (X11; Linux x86_64) orbital-populate/1.0"


def env_value(key: str) -> str:
    for line in ENV.read_text().splitlines():
        line = line.strip()
        if line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        if k.strip() == key:
            return v.strip().strip('"').strip("'")
    raise SystemExit(f"{key} missing from {ENV}")


class Client:
    def __init__(self, base: str) -> None:
        self.base = base
        self.cookie = ""

    def call(self, method: str, path: str, body: dict | None = None) -> tuple[int, dict]:
        req = urllib.request.Request(
            self.base + path,
            method=method,
            data=json.dumps(body).encode() if body is not None else None,
            headers={"Content-Type": "application/json", "User-Agent": UA, "Cookie": self.cookie},
        )
        try:
            with urllib.request.urlopen(req) as r:
                data = json.loads(r.read() or b"{}")
                for c in r.headers.get_all("set-cookie") or []:
                    if c.startswith("orbital_s="):
                        self.cookie = c.split(";")[0]
                return r.status, data
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read() or b"{}")

    def login(self, handle: str, password: str) -> None:
        status, body = self.call("POST", "/api/auth/login", {"handle": handle, "password": password})
        if status != 200:
            raise SystemExit(f"login failed for {handle}: {status} {body}")


# SFW only. Sites whose content is adult (whitebois, suki, stashdb, bnwo-feed,
# streakforge, blackrep) are listed on the steel titan account instead — a link
# to one of them from the SFW profile is the exact cross-pollination the
# segregation exists to prevent.
CRITICAL = {
    "handle": "criticalnexus",
    "password_key": "ORBITAL_CRITICALNEXUS_PASSWORD",
    "profile": {
        "display_name": "Critical Nexus",
        "accent": "#7c5cff",
        "bio": (
            "I build things and keep them running on one machine in my flat: a fanfiction "
            "archive you can keep offline, a recommender over that archive, a card game of "
            "powers, a forge for decisions, Bluesky feeds from a hand-picked curate list, a "
            "federated forum, and this directory.\n\n"
            "No ads, no tracking, no growth team. Software I wanted to exist. All of it is "
            "open source."
        ),
        "tags": ["atproto", "bluesky", "fanfiction", "indie-web", "open-source", "rust", "self-hosting"],
        "primary_vertical": "dev",
        "secondary_verticals": ["indie-web", "writers"],
    },
    "links": [
        ("https://news.polarisocial.xyz", "News — the feed reader", "rss"),
        ("https://darkhold.polarisocial.xyz", "Darkhold — fanfiction archive", "film"),
        ("https://kindred.polarisocial.xyz", "Kindred — fiction recommender", "link"),
        ("https://ascend.polarisocial.xyz", "Ascend — draw your powers", "gamepad"),
        ("https://concord.polarisocial.xyz", "Concord — the decision forge", "link"),
        ("https://gravity.polarisocial.xyz", "Gravity — federated social", "rss"),
        ("https://radio.polarisocial.xyz", "Radio", "music"),
        ("https://leftist.polarisocial.xyz", "Leftist feeds", "rss"),
        ("https://git.polarisocial.xyz", "The source (Forgejo)", "code"),
        ("https://bsky.app/profile/criticalnexus.bsky.social", "Bluesky", "link"),
    ],
}

STEELTITAN = {
    "handle": "steeltitan",
    "password_key": "ORBITAL_STEELTITAN_PASSWORD",
    "profile": {
        "display_name": "Steel Titan",
        "accent": "#f25aa9",
        "bio": (
            "Adult work, listed separately and deliberately: same machine, its own door. "
            "The SFW half of what I make is at Critical Nexus."
        ),
        "tags": ["adult", "booru", "feeds", "self-hosted"],
        "primary_vertical": "nsfw",
        "secondary_verticals": [],
    },
    "links": [
        ("https://bnwo-feed.polarisocial.xyz", "BNWO feed", "rss"),
        ("https://stashdb.polarisocial.xyz", "StashDB", "film"),
        ("https://suki.polarisocial.xyz", "Suki — booru", "brush"),
        ("https://whitebois.polarisocial.xyz", "Whitebois — booru", "brush"),
    ],
}


def populate(spec: dict) -> None:
    c = Client(BASE)
    c.login(spec["handle"], env_value(spec["password_key"]))
    status, me = c.call("GET", "/api/me")
    if status != 200:
        raise SystemExit(f"GET /api/me failed: {status} {me}")

    have = {l["url"] for l in me.get("links", [])}
    added = 0
    for url, label, icon in spec["links"]:
        if url in have:
            continue
        st, body = c.call("POST", "/api/links", {"url": url, "label": label, "icon": icon})
        if st not in (200, 201):
            print(f"  ! {url}: {st} {body.get('error', {}).get('message', body)}")
            continue
        added += 1

    st, body = c.call("PATCH", "/api/me", spec["profile"])
    if st != 200:
        raise SystemExit(f"profile patch failed: {st} {body}")

    st, me = c.call("GET", "/api/me")
    st2, standing = c.call("GET", "/api/me/standing")
    print(
        f"@{spec['handle']}: {len(me['links'])} links ({added} new), "
        f"verticals={[v['slug'] for v in me['verticals']]}, "
        f"trust={me['user']['trust_level']}, "
        f"standing={[(s['vertical'], s['top_rank'], s['members']) for s in standing.get('standing', [])]}"
    )


if __name__ == "__main__":
    for spec in (CRITICAL, STEELTITAN):
        populate(spec)
    print("done")
    sys.exit(0)
