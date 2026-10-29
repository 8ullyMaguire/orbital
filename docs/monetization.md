# Monetisation — orbital

Programme spec: `~/secondbrain/10-Projects/2026-10-29-estate-monetization-spec.md`
Phases and order: `~/secondbrain/10-Projects/2026-10-29-monetization-roadmap.md`

## What the traffic is, measured

The estate now has a pageview denominator: `/var/log/nginx/estate.json.log` (JSON `log_format` with
`$host`, added 2026-10-29) read by `~/estate-gates/pageviews.sh`. Measured on 2026-10-29:

```
~/estate-gates/pageviews.sh 7
  git.polarisocial.xyz        674 pageviews
  kindred.polarisocial.xyz    276
  leftist.polarisocial.xyz     10
  blog.polarisocial.xyz         2
  TOTAL                       962   → $0.96 at $1.00 RPM
```

**Orbital's own number: 12 requests today, none of them a pageview** — they are `/status`, `/og/…`
and `/api/*` from the deploy and the smoke test. The directory has four members and no audience yet.

So the honest statement is: at ~0 pageviews, every ad model pays zero. What follows is design, and it
should not be built before step 5 in the checklist below.

## This product

Server-rendered, no client beacons, and a footer that says: *"a directory, not an algorithm. No ads,
no trackers, no third-party scripts."* That sentence is the real constraint — the first slot either
changes it in the same commit or makes the site a liar.

- **Model:** a direct **sponsor row** on the directory index, at most one sponsor unit on a profile.
  Not a network banner: at this traffic a network pays cents, and a network script is the one thing
  the footer promises not to load.
- **Eligibility:** the adult vertical is served from the same origin (D9), so a mainstream network on
  a shared template is an account risk. Adult inventory, if it ever exists, is a separate account on
  a separate surface, never the same publisher ID.
- **Why not membership:** a directory has nothing to sell a member. If a tier ever exists it is
  "featured placement, clearly labelled", which is a sponsor row by another name.

## Where the code would go

- `templates/base.html` — the one place chrome is inserted (footer, or above `<main>`).
- `templates/home.html` — the sponsor row between the vertical grid and the trending rail.
- `templates/profile.html` — one unit under the links, never above them.
- `src/config.rs` — `ads_enabled: bool`, `ads_client: String`. Neither exists; adding a switch with
  no reader is the mistake `config-switches-have-readers` exists to prevent, so the switch and its
  reader land together.
- `static/ads.txt` — served at the apex for both hosts, if a network is ever used.
- `templates/base.html` footer — the "no ads, no trackers" sentence, in the same commit as the slot.

## Rules that are not negotiable

- No slot before an age gate, on an error page, or on any `/api/*` path.
- Reserved height for every slot; CLS stays 0. A banner that pushes text is worse than no banner.
- Consent before any personalised ad; non-personalised is the default, not the fallback.
- `rel="sponsored nofollow"` on anything paid.
- No anti-adblock wall, ever.
- Adult inventory on adult networks only, separate account, separate contact identity.

## Acceptance checklist, before a single slot ships

- [ ] Per-host pageviews for a full week — **done for the estate**, and orbital's own count is 0.
- [ ] A slot with a reserved height, and a visual diff of the profile at 1280×720 showing no shift.
- [ ] The footer sentence updated in the same commit as the first slot.
- [ ] `ads_enabled = 0` renders nothing at all — verified on the live host, not locally.
- [ ] `ads.txt` at the apex returns 200 with the right publisher line.
- [ ] No request to a third-party origin on any page with the switch off (DevTools, live).

## Falsification

If orbital is still under ~1,000 pageviews/month after a month of measurement, the answer is not a
network and not a tier: the estate has a traffic problem, and the only branch that pays without
growth is a named sponsor on a page five people read — worth what it is worth, and no more.
