#!/usr/bin/env bash
# The silence gate, against the deployed host: no surface a stranger can reach may name the prior.
#
# The vocabulary is the crate's (`src/silence.rs`), repeated here because this script runs with nothing but
# curl: a gate that needs a build is a gate that runs when somebody remembers.
#
# One surface may say the word and only in one way: the estate's capabilities document declares
# `"taste":"off"` — the family's name for a capability this instance does not offer. It is checked
# separately so that any *other* use of the word anywhere fails the run.
set -uo pipefail

BASE="${1:-https://orbital.polarisocial.xyz}"
UA="curl/8.6.0"
WORDS='gravity|prior|taste|attuned|affinity|taste-source|instance lens'
fail=0

say() { printf '  %-26s %s\n' "$1" "$2"; }

for path in / /about /v/dev "/v/dev?tab=trending" "/search?q=rust" /api/v1/members \
            /api/v1/stats /api/v1/verticals "/api/v1/search?q=rust" /api/v1/ring/dev \
            /feed.xml /feed.json /sitemap.xml /robots.txt; do
  body=$(curl -s -H "User-Agent: $UA" "$BASE$path") || { say "$path" "FETCH FAILED"; fail=1; continue; }
  hits=$(printf '%s' "$body" | grep -oiE "\b($WORDS)\b" | sort -u | paste -sd, -)
  if [ -n "$hits" ]; then
    say "$path" "NAMES THE PRIOR: $hits"
    fail=1
  else
    say "$path" "silent"
  fi
done

# The exemption, exactly one field and exactly one value.
caps=$(curl -s -H "User-Agent: $UA" "$BASE/api/v1/capabilities")
if printf '%s' "$caps" | grep -q '"taste":"off"'; then
  stray=$(printf '%s' "$caps" | sed 's/"taste":"off"//g' | grep -oiE "\b($WORDS)\b" | sort -u | paste -sd, -)
  if [ -n "$stray" ]; then
    say "/api/v1/capabilities" "NAMES THE PRIOR BEYOND taste:off: $stray"
    fail=1
  else
    say "/api/v1/capabilities" "silent but for taste:off"
  fi
else
  say "/api/v1/capabilities" "does not declare taste at all (fine, and nothing to exempt)"
fi

# And the one route that may use the word must answer about the caller, never about the instance.
lens=$(curl -s -H "User-Agent: $UA" "$BASE/api/v1/taste")
if printf '%s' "$lens" | grep -q '"dial"'; then
  stray=$(printf '%s' "$lens" | grep -oiE "\b($WORDS)\b" | sort -u | paste -sd, -)
  if [ -n "$stray" ]; then
    say "/api/v1/taste" "answers more than the caller's lens: $stray"
    fail=1
  else
    say "/api/v1/taste" "caller's lens only"
  fi
else
  say "/api/v1/taste" "MISSING or not a lens answer: ${lens:0:80}"
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "FAIL silence: a public surface names the prior"
  exit 1
fi
echo "OK silence: every public surface is silent"
