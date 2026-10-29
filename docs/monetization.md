# Monetisation — orbital

Programme spec: `/home/alvaro/secondbrain/10-Projects/2026-10-29-estate-monetization-spec.md`
Phases and order: `/home/alvaro/secondbrain/10-Projects/2026-10-29-monetization-roadmap.md`

## This product

Sponsor row (direct) on the directory index + one adult-unit on profile pages. Server-rendered, so this is the pilot.

## Where the code goes

* `templates/base.html — head/nav/footer, the one place a slot is inserted`
* `templates/ — vertical index and profile templates`
* `src/config.rs — ads_enabled, ads_client`
* `static/ads.txt served at the apex for both orbital and orbitalnsfw`

## Known gap

None blocking: nginx pageviews are exact for server-rendered pages.

## Rules that are not negotiable (full list in the programme spec)

* No ad before an age gate, on an error page, or on any `/api/*` path — the estate's no-trap gate checks
  the last one.
* Every slot has a reserved height; CLS stays at 0. A banner that pushes text is worse than no banner.
* Consent before any personalised ad; non-personalised is the default, not the fallback.
* Adult inventory uses adult networks only. A mainstream network on this surface ends that account.

## Measurement

`~/estate-gates/pageviews.sh` on thinkcentre reports this host's pageviews; `rpm.py` prints RPM against
`revenue.csv`. Nothing here counts as measured until both are non-zero for a full week.
