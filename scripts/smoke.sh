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

# ---- the tuner (SPEC v3)
# Over HTTP the visitor cookie is discarded (production sets `Secure`, and curl will not store a
# Secure cookie from a plain-http response), so every request would be a new visitor and a check
# here would pass without testing anything. That is why this section talks to $PUBLIC.
PUBLIC="${PUBLIC:-https://orbital.polarisocial.xyz}"
body "$BASE/" | grep -q 'What do you want to discover?' \
  && pass "the tuner is on the directory" || fail "tuner" "widget missing from /"

body "$BASE/v/dev" | grep -q 'action="/prefs"' \
  && pass "the tuner posts a form (works without JavaScript)" || fail "tuner form" "no /prefs action"

if [ "$(curl -s -o /dev/null -w '%{http_code}' "$PUBLIC/healthz" 2>/dev/null)" = "200" ]; then
  cj="$JAR.tuner"
  curl -s -c "$cj" "$PUBLIC/" >/dev/null
  c=$(curl -s -o /dev/null -w '%{http_code}' -b "$cj" -X POST \
        -H "Content-Type: application/x-www-form-urlencoded" \
        --data 'tag=chiptune&w=10' "$PUBLIC/prefs")
  if [ "$c" = "303" ] || [ "$c" = "302" ]; then
    pass "POST /prefs accepts a weight"
  else
    fail "prefs post" "status $c"
  fi
  # The weight must come back: a 303 alone only proves the form parsed.
  if curl -s -b "$cj" "$PUBLIC/lens" | grep -q 'chiptune'; then
    pass "the weight persists and is readable at /lens"
  else
    fail "prefs persist" "the lens page does not show the weight"
  fi
  curl -s -b "$cj" -X POST "$PUBLIC/prefs/reset" >/dev/null
else
  printf 'SKIP  the tuner persistence check (%s unreachable from here)\n' "$PUBLIC"
fi

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
# One fixed account, reused. A timestamped handle per run left a new member in the live directory
# on every deploy - the smoke test was quietly seeding the product with test pilots.
H="smoketester"
PW="correct horse battery"
reg=$(curl -s -c "$JAR" -X POST "$BASE/api/auth/register" -H 'content-type: application/json' \
  -d "{\"handle\":\"$H\",\"password\":\"$PW\",\"display_name\":\"Smoke Tester\",\"vertical\":\"dev\",\"bio\":\"I test things end to end.\"}")
if echo "$reg" | grep -q '"handle":"'"$H"'"'; then
  pass "register"
else
  # Already exists from a previous run: sign in instead of creating a second one.
  login=$(curl -s -c "$JAR" -X POST "$BASE/api/auth/login" -H 'content-type: application/json' \
    -d "{\"handle\":\"$H\",\"password\":\"$PW\"}")
  echo "$login" | grep -q '"handle":"'"$H"'"' \
    && pass "login (account already existed)" || fail "register" "${reg:0:160}"
fi

# The fixture is a real member in a real directory. It un-hides itself to be tested and hides
# itself again at the end, so the product is not seeded with a test pilot between deploys.
curl -s -o /dev/null -b "$JAR" -X PATCH "$BASE/api/me" -H 'content-type: application/json' -d '{"hidden":false}'

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

# ---- platform surfaces (2026-10-29)
c=$(code "$BASE/status")
[ "$c" = 200 ] && pass "estate status page" || fail "status page" "status $c"
curl -s "$BASE/status" | grep -q "hosts answering" \
  && pass "status page counts the estate" || fail "status page" "no summary line"
curl -s "$BASE/og/$H.svg" | grep -q "<svg" \
  && pass "a social card is rendered" || fail "og" "no svg for $H"
curl -s -H "Authorization: Bearer orb_nope" "$BASE/api/v1/me/links" -o /dev/null -w '%{http_code}' \
  | grep -q '^401$' && pass "a bad token is refused" || fail "token auth" "not 401"

# Leave the directory as we found it: hidden, not deleted.
curl -s -o /dev/null -b "$JAR" -X PATCH "$BASE/api/me" -H 'content-type: application/json' -d '{"hidden":true}'
c=$(code "$BASE/@$H")
[ "$c" = 404 ] && pass "the fixture hides itself again" || fail "fixture" "profile still $c after hiding"

printf '\n%d failure(s)\n' "$FAILS"
exit "$FAILS"
