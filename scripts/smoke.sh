#!/usr/bin/env bash
# End-to-end smoke against a running Orbital. Every check prints PASS/FAIL and
# a one-line reason; the exit status is the number of failures.
#
#   BASE=http://127.0.0.1:8090 NSFW_HOST=nsfw.localhost ./scripts/smoke.sh
set -uo pipefail

BASE="${BASE:-http://127.0.0.1:8090}"
NSFW_HOST="${NSFW_HOST:-nsfw.localhost}"
JAR="$(mktemp -d)/jar"
FAILS=0

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s  (%s)\n' "$1" "$2"; FAILS=$((FAILS + 1)); }

code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
body() { curl -s "$@"; }

# ---- public pages
for p in / /about /v/dev /search /feed.json /feed.xml /sitemap.xml /robots.txt /assets/site.css /assets/og.png /favicon.ico; do
  c=$(code "$BASE$p")
  [ "$c" = 200 ] && pass "GET $p -> 200" || fail "GET $p" "status $c"
done

c=$(code "$BASE/@criticalnexus")
[ "$c" = 200 ] && pass "profile renders" || fail "profile" "status $c"

body "$BASE/@criticalnexus" | grep -q 'distinct visitors' \
  && pass "profile carries measured stats" || fail "profile stats" "no counters"

body "$BASE/@criticalnexus" | grep -q 'application/ld+json' \
  && pass "profile carries JSON-LD" || fail "jsonld" "missing"

body "$BASE/@criticalnexus" | grep -q 'https://leftist.polarisocial.xyz' \
  && fail "link target" "outbound URL is untracked, the click would be lost" \
  || pass "links are source-tagged"

c=$(code "$BASE/@steeltitan")
[ "$c" = 200 ] && pass "adult profile is on the one surface" || fail "one surface" "status $c"

body "$BASE/sitemap.xml" | grep -q 'nsfw' \
  && pass "sitemap lists the adult vertical" || fail "sitemap" "the nsfw vertical is missing"

# ---- auth + links
H="testpilot$(date +%s)"
reg=$(curl -s -c "$JAR" -X POST "$BASE/api/auth/register" -H 'content-type: application/json' \
  -d "{\"handle\":\"$H\",\"password\":\"correct horse battery\",\"display_name\":\"Test Pilot\",\"vertical\":\"dev\",\"bio\":\"I test things end to end.\"}")
echo "$reg" | grep -q '"handle":"'"$H"'"' && pass "register" || fail "register" "${reg:0:160}"

c=$(code -b "$JAR" "$BASE/api/me")
[ "$c" = 200 ] && pass "session cookie works" || fail "session" "status $c"

link=$(curl -s -b "$JAR" -X POST "$BASE/api/links" -H 'content-type: application/json' \
  -d '{"url":"https://example.com/end-to-end","label":"E2E link"}')
LID=$(printf '%s' "$link" | python3 -c 'import json,sys; print(json.load(sys.stdin)["link"]["id"])' 2>/dev/null)
[ -n "${LID:-}" ] && pass "create link" || fail "create link" "${link:0:160}"

# The card's href is /l/<id> (a tracked redirect) and the visible text is the label plus the
# URL's host-and-path in <small>. Assert both, because either one alone can be true of a
# different card.
page=$(body "$BASE/@$H")
printf '%s' "$page" | grep -q 'E2E link' \
  && printf '%s' "$page" | grep -q 'example.com/end-to-end' \
  && pass "link appears on the public profile" || fail "public link" "not rendered"

# A credentials-bearing URL must be refused.
c=$(code -b "$JAR" -X POST "$BASE/api/links" -H 'content-type: application/json' \
  -d '{"url":"https://user:pw@example.com/x"}')
[ "$c" = 400 ] && pass "credentialed URL refused" || fail "url guard" "status $c"

# ---- click tracking
loc=$(curl -s -o /dev/null -D - -b "$JAR" "$BASE/l/$LID?s=vertical&c=dev&p=1" | grep -i '^location:' | tr -d '\r')
echo "$loc" | grep -q 'example.com/end-to-end' && pass "click redirects to the target" || fail "click" "$loc"

sleep 0.3
an=$(body -b "$JAR" "$BASE/api/analytics?days=7")
echo "$an" | python3 -c '
import json,sys
d=json.load(sys.stdin)
t=d["totals"]
assert t["clicks"] >= 1, d
' 2>/dev/null && pass "click is in the member's analytics" || fail "analytics" "${an:0:200}"

body "$BASE/v/dev?tab=top" | grep -q "$H" && pass "member is on the vertical leaderboard" || fail "leaderboard" "not listed"
body "$BASE/v/dev?tab=trending" | grep -q "$H" && pass "member is on trending" || fail "trending" "not listed"

# ---- one surface (DECISIONS D9): the adult vertical is an ordinary category
c=$(code "$BASE/@steeltitan")
[ "$c" = 200 ] && pass "adult profile renders on the one host" || fail "adult profile" "status $c"
c=$(code "$BASE/v/nsfw")
[ "$c" = 200 ] && pass "the nsfw vertical has a page" || fail "nsfw vertical" "status $c"
c=$(code "$BASE/gate")
[ "$c" = 404 ] && pass "the gate is retired" || fail "gate" "status $c"
# In production nginx answers the retired name with a 301; with no proxy in front the app
# serves the same page. What must not happen is a different surface (403/404) or an error.
c=$(code -H "Host: $NSFW_HOST" "$BASE/@steeltitan")
case "$c" in 200|301|302|303|307|308) pass "the retired hostname is not a second surface" ;; *) fail "host split" "status $c" ;; esac
body "$BASE/sitemap.xml" | grep -q '/v/nsfw' \
  && pass "the sitemap lists the adult vertical" || fail "sitemap" "nsfw vertical missing"
curl -sI "$BASE/" | grep -qi 'x-robots-tag' \
  && fail "robots" "a page still carries X-Robots-Tag" || pass "no X-Robots-Tag anywhere"
body "$BASE/feed.json" | grep -qi 'adult' \
  && pass "the feed carries the whole directory" || fail "feed" "adult vertical filtered out"

# ---- dashboard + reports
c=$(code "$BASE/app/")
[ "$c" = 200 ] && pass "dashboard shell" || fail "dashboard" "status $c"
c=$(code -X POST "$BASE/report" -d "handle=$H&reason=other&detail=smoke")
[ "$c" = 200 ] && pass "report form" || fail "report" "status $c"

printf '\n%d failure(s)\n' "$FAILS"
exit "$FAILS"
