# Orbital — roadmap

State at the time of writing: **schema v5**, 91 tests (43 unit, 41 HTTP, 7 store), `orbital doctor`
green on the host, `scripts/smoke.sh` green, seven timers running.

This file is the honest list. "Done" means deployed and verified against the live site, not merged.

---

## Done

**The product**

- Profiles, links, tags, one primary vertical and two secondaries; ranked Top / Trending / New /
  Curated per vertical, webring, feeds, embeddable leaderboard, sitemap and robots.
- Public pages are server-rendered (askama); the dashboard is a Svelte bundle embedded in the binary.
- Click attribution through `/l/{id}` with referrer host only; distinct-session visitor counting.
- Link health (checker, badges at three failures, SSRF guard including redirect hops).
- One surface (D9): the adult vertical is a vertical, `orbitalnsfw` 301s to the canonical origin.
- Personalisation (SPEC v3): a lens per member or per anonymous visitor, passive accumulation with a
  five-page warm-up, `/lens` to read and edit it, an off switch, and adoption of the anonymous lens
  at signup. Weights capped at 24.
- Governance: proposals, three-vote quorum, hand decision, merges that move members with their tags,
  and TL2+ endorsements worth up to +0.2 relevance as their own term.
- API tokens and a write API; `rank-delta` from weekly snapshots.
- Fetched page titles and favicons; generated social cards (PNG via rsvg-convert, SVG fallback).
- `/status` — the estate as nginx actually serves it.
- English and Spanish by cookie or `Accept-Language`, with the chrome translated.

**The operations**

- `orbital doctor` (exit code = failures), `import`, `snapshot`, `retag`, `fetch-titles`, `og`,
  `prune`, `check-links`.
- Seven user timers: checklinks, backup, restore-test, prune, snapshot, metadata+cards, uptime.
- Nightly dumps to `.part` then rename, 14 kept, monthly restore test into a scratch database.
- nginx rate limits on the API and on the unauthenticated write path; CORS on the read API; no
  `Set-Cookie` on API responses so the edge cache is honest.
- CI on both forges running the suite against a Postgres service.
- Smoke test that hides its own fixture, so the directory is not seeded with a test pilot.

---

## Next, in order

1. **Page bodies into the dictionary.** The mechanism and the chrome are done; home, about, vertical,
   profile, lens, status and search still carry literal English. The key-existence test makes the
   remaining work enumerable: add strings to `src/i18n.rs`, replace literals with `layout.t("…")`.
   Do one template per commit so a half-done template is never on `main`.
2. **MCP write tools.** The API half of "script your own links" is live (`POST /api/v1/me/links` with
   a Bearer token); `mcp/orbital_mcp.py` still exposes only the seven read tools. `ORBITAL_TOKEN`
   in the environment, three tools, and the read-only note in the README disappears.
3. **Real profiles.** The directory has four members and `orbital import` exists. This is not a code
   task; five people with a URL each is what makes the thing a directory.
4. **A digest that posts.** `bot spotlight`, `bot roundup` and `bot maintain` exist; the weekly
   dead-link digest does not. It needs a decision about where it posts more than it needs code.
5. **Endorsements on the page.** The API and the ranking effect exist; nothing in the UI lets a
   member endorse a tag they did not type themselves. Small, and it makes the feature discoverable.

---

## Deliberately not done

- **Seeding members.** Inventing people to fill a directory of real people is the one shortcut that
  would make the product a lie. `orbital import` plus five humans instead.
- **Email, OAuth, custom domains per member.** D7, and the estate has no SMTP transport.
- **A quorum for flag adjudication.** D4: three accounts could remove a fourth.
- **Ads.** `docs/monetization.md` holds the plan and the non-negotiables (reserved height, no
  personalised ad before consent, adult inventory on adult networks only). Nothing is implemented,
  and `ads_enabled` / `ads_client` do not exist yet.
- **Machine-translating the remaining templates.** The dictionary is hand-written on purpose; a
  wrong translation in a directory of people's names is worse than an English sentence.

---

## Known risks

- **Cloudflare in front of everything.** `$remote_addr` is an edge address, so every rate limit keys
  on `CF-Connecting-IP`; a request that bypasses the tunnel is keyed on the socket instead. If that
  header is ever absent for real traffic, every visitor shares one bucket.
- **Passive personalisation is a cookie.** Someone who clears cookies becomes a new visitor; the
  prune job forgets profiles after 180 days. That is intended, and it is also why the tuner exists.
- **The link checker is the only thing standing between a member and a dead link.** It runs daily;
  a link that dies at 04:21 is offered to visitors for almost a day, with a badge after three checks.
- **`static/estate.txt` is generated.** If the deploy's sed ever matches nothing, `/status` reads
  "0 of 0" rather than lying about a network it cannot see.
