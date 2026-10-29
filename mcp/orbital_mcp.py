#!/usr/bin/env python3
"""Orbital MCP server — the directory's public read API as MCP tools.

Two decisions worth stating up front:

* **It talks HTTP, not SQL.** The server is a thin client of `/api/v1`, so it can run anywhere the
  site is reachable, needs no database credentials, and cannot drift from the app's own rules about
  what is public. If a tool here shows something the site would not show, that is a bug in the API,
  not a hint in the client.
* **Read-only by construction.** There is no tool that writes. Writing needs a member token, and
  `api_tokens` does not exist yet, so a write tool would have to carry the author's session cookie —
  which is exactly the thing an MCP server must not hold.

Run it over stdio:

    python3 orbital_mcp.py

Environment:
    ORBITAL_BASE   origin of the site (default https://orbital.polarisocial.xyz)
    ORBITAL_TIMEOUT seconds per request (default 15)
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

try:  # mcp 2.x renamed FastMCP to MCPServer and changed nothing we use
    from mcp.server.mcpserver import MCPServer as _Server
except ImportError:  # mcp < 2
    from mcp.server.fastmcp import FastMCP as _Server

BASE = os.environ.get("ORBITAL_BASE", "https://orbital.polarisocial.xyz").rstrip("/")
TIMEOUT = float(os.environ.get("ORBITAL_TIMEOUT", "15"))

mcp = _Server("orbital", instructions=(
    "Read-only access to the Orbital directory: members, categories, links and link health. "
    "Start with orbital_stats, then orbital_members(vertical=...) to browse."
))


def _get(path: str, params: dict | None = None) -> dict:
    """One request, one dict. Errors are returned as data, not raised: an MCP client shows the
    string either way, and a traceback tells it nothing about what to do next."""
    url = f"{BASE}{path}"
    if params:
        clean = {k: v for k, v in params.items() if v is not None}
        if clean:
            url += "?" + urllib.parse.urlencode(clean)
    req = urllib.request.Request(url, headers={"accept": "application/json",
                                               "user-agent": "orbital-mcp/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", "replace")[:300]
        return {"error": f"HTTP {e.code} from {path}", "detail": body}
    except Exception as e:  # noqa: BLE001 - transport failures are data here
        return {"error": f"{type(e).__name__}: {e}", "url": url}


@mcp.tool()
def orbital_stats() -> str:
    """Directory totals: members, verticals, clicks, visitors, and link health counts.

    Use this first: it answers "how big is this directory" without fetching every member, and the
    link counts distinguish a directory with no dead links from one where nothing has been checked.
    """
    return json.dumps(_get("/api/v1/stats"), indent=2)


@mcp.tool()
def orbital_verticals() -> str:
    """Every category with its slug, label, description and member count.

    The slug is the value to pass to orbital_members(vertical=...) or orbital_ring(vertical=...).
    """
    return json.dumps(_get("/api/v1/verticals"), indent=2)


@mcp.tool()
def orbital_members(vertical: str | None = None, q: str | None = None,
                    limit: int = 25, offset: int = 0) -> str:
    """List members, optionally filtered by vertical slug or a free-text query.

    Args:
        vertical: category slug from orbital_verticals, e.g. "dev" or "writing".
        q: search term matched against handle, display name, bio and tags.
        limit: page size, 1-200 (default 25).
        offset: rows to skip, for paging.

    Returns each member's handle, url, verticals, tags, trust level and visitor/click counts. Rows
    are ordered by the same ranking the site shows, so the first page is the honest top.
    """
    return json.dumps(_get("/api/v1/members", {"vertical": vertical, "q": q,
                                               "limit": limit, "offset": offset}), indent=2)


@mcp.tool()
def orbital_profile(handle: str) -> str:
    """One member: bio, tags, Bluesky claim (and whether it was verified), and their links.

    Each link carries `failing` — true when the link checker has watched it fail three nights
    running — so callers can avoid recommending a dead site without asking a second endpoint.
    """
    return json.dumps(_get(f"/api/v1/members/{urllib.parse.quote(handle.lstrip('@'))}"), indent=2)


@mcp.tool()
def orbital_links(handle: str) -> str:
    """Just the outbound links of one member, with status, last check time and fail count."""
    return json.dumps(_get(f"/api/v1/members/{urllib.parse.quote(handle.lstrip('@'))}/links"), indent=2)


@mcp.tool()
def orbital_broken_links() -> str:
    """The links that have failed three or more consecutive checks, worst first, with the HTTP
    status and when they were last looked at. Empty list means every checked link answered.
    """
    return json.dumps(_get("/api/v1/links/health"), indent=2)


@mcp.tool()
def orbital_ring(vertical: str, from_handle: str | None = None) -> str:
    """The next member in a category's webring.

    Args:
        vertical: category slug.
        from_handle: whose neighbour to return; omit to start at the top of the ring.

    The ring wraps, and it is the same order the profile pages use, so walking it with
    `from_handle` set to each answer traverses the whole category exactly once.
    """
    return json.dumps(_get(f"/api/v1/ring/{urllib.parse.quote(vertical)}",
                           {"from": from_handle}), indent=2)


if __name__ == "__main__":
    print(f"[orbital-mcp] {BASE}", file=sys.stderr)
    mcp.run()
