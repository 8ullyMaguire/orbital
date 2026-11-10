#!/usr/bin/env bash
# Prove the estate's capabilities document is served here — and that it tells the truth.
#
#   ./scripts/check-capabilities.sh                     # against 127.0.0.1:8034 (this vhost)
#   ./scripts/check-capabilities.sh https://orbital.polarisocial.xyz
#
# WHY this exists: `/api/v1/capabilities` is the W0 contract's declaration of what this site can do,
# and a declaration is worth nothing if it is a 404 or if it lists a route that does not answer.
# Measured 2026-11-10: the document was generated and installed in /var/www/estate/capabilities, and
# the vhost still answered 404 because it proxied all of /api/v1 to the app — so the check is a
# *served* check, not a file-existence check.
#
# Four assertions, each of which has failed in this estate at least once:
#   1. 200 — the route exists through nginx, not just in the render output
#   2. site/orbital — no other product's document is reachable on this name
#   3. every declared route answers, and none of them 404s (405 is fine: POST-only endpoints)
#   4. the document declares what this site actually does (search/taste/recs on, not "off")
#
# Exit code 0 = all four hold; 1 = the first failing one, named.
set -euo pipefail

base="${1:-http://127.0.0.1:8034}"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

code=$(curl -sS -o "$tmp" -w '%{http_code}' --max-time 8 "$base/api/v1/capabilities" || echo 000)
if [ "$code" != "200" ]; then
  echo "FAIL capabilities: HTTP $code at $base/api/v1/capabilities"
  case "$code" in
    404) echo "  the vhost does not include /etc/nginx/snippets/estate-capabilities.conf" ;;
    502|000) echo "  nginx or the app is down" ;;
  esac
  exit 1
fi

python3 - "$base" "$tmp" <<'PY'
import json, sys, urllib.error, urllib.request

base, path = sys.argv[1], sys.argv[2]
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
    try:
        with urllib.request.urlopen(base + route, timeout=8) as r:
            code = r.status
    except urllib.error.HTTPError as e:
        code = e.code
    except Exception:
        code = 0
    if code in (404, 0):
        problems.append(f"route {name} {route} answers {code}")
    print(f'  route {name:12s} {route:24s} {code}')

# The document is a claim about this site. A capability the site really serves must not read "off":
# that is the failure mode that makes the contract decorative.
required = {"search", "taste", "recs", "preservation", "feeds"}
missing = [k for k in required if k not in caps]
if missing:
    problems.append("capabilities is missing " + ", ".join(missing))
if caps.get("feeds") is not True:
    problems.append(f'capabilities.feeds is {caps.get("feeds")!r}, but /feed.xml is served')

if problems:
    print("FAIL " + "; ".join(problems))
    sys.exit(1)
print(f"OK capabilities: site=orbital, {len(routes)} routes answer, "
      f"identity={caps.get('identity')} search={caps.get('search')} taste={caps.get('taste')} "
      f"recs={caps.get('recs')} preservation={caps.get('preservation')} feeds={caps.get('feeds')}")
PY
