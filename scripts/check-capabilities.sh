#!/usr/bin/env bash
# Prove the estate's capabilities document is served here — and that it tells the truth.
#
#   ./scripts/check-capabilities.sh                     # dials 127.0.0.1:8034, sends Host: orbital.polarisocial.xyz
#   ./scripts/check-capabilities.sh https://orbital.polarisocial.xyz
#   ./scripts/check-capabilities.sh http://127.0.0.1:8034 orbital.polarisocial.xyz
#
# WHY this exists: /api/v1/capabilities is the W0 contract's declaration of what this site can do,
# and a declaration is worth nothing if it is a 404 or if it lists a route that does not answer.
# Measured 2026-11-10: the document was generated and installed in /var/www/estate/capabilities and
# the vhost still answered 404 because it proxied all of /api/v1 to the app.
#
# WHY the Host header matters: nginx picks the document with try_files /$estate_site.json, and
# $estate_site comes from a map keyed by $host. A probe that dials 127.0.0.1 without a Host header
# gets an empty document name and a 404, so a gate that forgets it fails on a correct config.
# Measured 2026-11-10: this script reported 404 against a vhost that was right, one deploy after it
# was written. A gate is a program, and it has bugs like any other.
#
# Four assertions, each of which has failed in this estate at least once:
#   1. 200 — the route exists through nginx, not just in the render output
#   2. site = orbital — no other product's document is reachable on this name
#   3. every declared route answers, and none of them 404s (405 is fine: POST-only endpoints)
#   4. the document declares what this site actually serves
#
# Exit code 0 = all four hold; 1 = the first failing one, named.
set -euo pipefail

base="${1:-http://127.0.0.1:8034}"
host_header="${2:-}"
if [ -z "$host_header" ]; then
  case "$base" in
    *"://127.0.0.1"*|*"://localhost"*) host_header="orbital.polarisocial.xyz" ;;
  esac
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

hdr_args=()
if [ -n "$host_header" ]; then
  hdr_args=(-H "Host: $host_header")
fi

code=$(curl -sS "${hdr_args[@]}" -o "$tmp" -w '%{http_code}' --max-time 8 "$base/api/v1/capabilities" || echo 000)
if [ "$code" != "200" ]; then
  echo "FAIL capabilities: HTTP $code at $base/api/v1/capabilities (Host: ${host_header:-none})"
  case "$code" in
    404)
      echo "  either the vhost omits /etc/nginx/snippets/estate-capabilities.conf, or the Host header"
      echo "  is not in the estate map, or /var/www/estate/capabilities is missing the document"
      ;;
    502|000) echo "  nginx or the app is down" ;;
  esac
  exit 1
fi

python3 - "$base" "$tmp" "$host_header" <<'PY'
import json, sys, urllib.error, urllib.request

base, path, host = sys.argv[1], sys.argv[2], sys.argv[3]
doc = json.load(open(path))
problems = []

if doc.get("site") != "orbital":
    problems.append(f'site is {doc.get("site")!r}, expected "orbital"')
if doc.get("api") != "v1":
    problems.append(f'api is {doc.get("api")!r}, expected "v1"')
caps = doc.get("capabilities") or {}
federation = caps.get("federation")
if federation is not None and not isinstance(federation, list):
    problems.append("capabilities.federation must be an array")
routes = doc.get("routes") or {}
if "capabilities" not in routes:
    problems.append("routes.capabilities is missing")

for name, route in sorted(routes.items()):
    req = urllib.request.Request(base + route)
    # Cloudflare answers 403 to Python's default agent; a gate that reports 403 as "answers" is a
    # gate that never touched the origin (measured 2026-11-10: every route read 403 through the
    # public hostname).
    req.add_header("User-Agent", "curl/8.6.0")
    if host:
        req.add_header("Host", host)
    try:
        with urllib.request.urlopen(req, timeout=8) as r:
            code = r.status
    except urllib.error.HTTPError as e:
        code = e.code
    except Exception:
        code = 0
    if code in (404, 403, 0):
        problems.append(f"route {name} {route} answers {code}")
    print(f"  route {name:12s} {route:24s} {code}")

# The document is a claim about this site: a declared capability must have something behind it.
for key in ("identity", "search", "recs", "feeds"):
    if key not in caps:
        problems.append(f"capabilities.{key} is missing")
if caps.get("feeds") is not True:
    problems.append(f'capabilities.feeds is {caps.get("feeds")!r}, but /feed.xml is served')
if caps.get("search") != "off" and "search" not in routes:
    problems.append("search is declared on but no search route is declared")

if problems:
    print("FAIL " + "; ".join(problems))
    sys.exit(1)
print(f"OK capabilities: site=orbital, {len(routes)} routes answer, "
      f"identity={caps.get('identity')} search={caps.get('search')} taste={caps.get('taste')} "
      f"recs={caps.get('recs')} preservation={caps.get('preservation')} feeds={caps.get('feeds')}")
PY
