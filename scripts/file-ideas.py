#!/usr/bin/env python3
"""Generate the SQL that files the 2026-11-10 orbital brainstorm into Concord project 'orbital'.

Source of truth: ~/secondbrain/10-Projects/orbital-ideas-2026-11-10.md (496 numbered ideas).
Output: /home/alvaro/.hermes/profiles/coding/cache/scratch/orbital-filing.sql (idempotent).

Shape per idea, matching what project 73 already carries (27 features = 27 complaints = 27 cards):
  complaint -> feature -> feature_complaints -> board_cards -> arena_entries
Project resolved by SLUG inside the SQL; numeric ids are per-instance.
"""
import re, sys, collections

SRC = "/home/alvaro/secondbrain/10-Projects/orbital-ideas-2026-11-10.md"
OUT = "/home/alvaro/.hermes/profiles/coding/cache/scratch/orbital-filing.sql"
DISP = "/home/alvaro/secondbrain/10-Projects/orbital-ideas-disposition.md"
SLUG = "orbital"

# ---- section -> (impact 1-10, effort weeks 1-10) ---------------------------------
SECTION_SCORE = {
    "I.A": (8, 3), "I.B": (7, 4), "I.C": (8, 2), "I.D": (6, 2),
    "II.E": (7, 3), "II.F": (8, 2), "II.G": (7, 2), "II.H": (7, 3), "II.I": (8, 2), "II.J": (6, 2),
    "III.K": (5, 2), "III.L": (4, 3), "III.M": (5, 2), "III.N": (7, 2),
    "IV.O": (4, 2), "IV.P": (4, 2), "IV.Q": (5, 2),
    "V.R": (9, 3), "V.S": (7, 2), "V.T": (6, 2),
    "VI.U": (5, 1), "VI.V": (6, 2), "VI.W": (5, 1),
    "VII.X": (6, 2), "VII.Y": (8, 4), "VII.Z": (6, 2),
    "VIII.AA": (4, 2), "VIII.BB": (5, 2),
    "IX.CC": (7, 2),
    "X.DD": (6, 2), "X.EE": (5, 2), "X.FF": (4, 2),
    "XI.GG": (3, 4),
}

# ---- the owner's dismissals, each with the reason the page records ----------------
REJECTED = {
    # gravity disclosure: the admin's taste is applied and never named. docs only.
    62:  ("rejected", "Disclosure", "A distinct visual treatment for the operator's endorsement tells a reader which taste the ranking came from. The prior is applied and never named: no field, page, sentence, URL or badge."),
    91:  ("rejected", "Disclosure", "A labelled staff-pick shelf is the prior wearing a name. Shelves may exist; none may be attributable to the operator's taste."),
    104: ("rejected", "Disclosure", "A cold visitor is ranked by the instance vector, but the page may not say so or explain 'what this place is about'. Application yes, announcement no."),
    171: ("rejected", "Disclosure", "A visible taste vector is the policy itself, and two responses that differ by one candidate recover it by arithmetic."),
    173: ("rejected", "Disclosure", "Visual distinction for the operator's reviews announces whose taste is privileged."),
    174: ("rejected", "Disclosure", "A 'curated by Orbital' badge is a per-entity disclosure of the prior's scoring."),
    176: ("rejected", "Disclosure", "A 'why' line naming the instance's collective taste is the prior in prose. `why` lines may cite clicks, recency, tags and endorsements only."),
    177: ("rejected", "Disclosure", "A public manifesto is the prior written out; docs/ carries the policy, the site carries none of it."),
    178: ("rejected", "Disclosure", "Publishing the taste vector over ActivityPub hands every peer the policy."),
    181: ("rejected", "Disclosure", "Instance-to-instance taste comparison publishes both vectors."),
    183: ("rejected", "Disclosure", "A per-entity 'how Orbital sees this' score is the prior's contribution on a single row."),
    184: ("rejected", "Disclosure", "A public /@orbital profile with its endorsements and vector is the disclosure with a name and an avatar."),
    222: ("rejected", "Disclosure", "One-click 'adopt the operator's taste' both reveals that such a taste exists and hands it over."),
    377: ("rejected", "Disclosure", "Instance-to-instance taste sharing — same as 178, over a profile attachment."),
    380: ("rejected", "Disclosure", "Publishing 'this instance disagrees with that instance' is a per-entity disclosure at federation scale."),
    385: ("rejected", "Disclosure", "A federation map of instances' taste profiles is the disclosure rendered as a chart."),
    475: ("rejected", "Disclosure", "Cross-instance taste gravity publishes the vectors it pulls with."),
    # breadth: depth only, plus ActivityPub and power-user surfaces
    476: ("rejected", "Breadth", "A recommendation API for third parties is a second product. The estate already has one index; orbital is a client, not a vendor."),
    478: ("rejected", "Breadth", "A time-capsule reminder is notifications and scheduling for a site with 126 visitors; it needs a mail path orbital deliberately does not have."),
    479: ("rejected", "Breadth", "Review translation is a machine-translation surface; page bodies stay English and chrome is already translated."),
    480: ("rejected", "Breadth", "Voice reviews need storage, transcription and moderation for a corpus that has almost no text reviews yet."),
    482: ("rejected", "Breadth", "Private team instances are a different product (and the estate already runs one per site)."),
    483: ("rejected", "Breadth", "Reading challenges are gamification; polaris-loops owns that noun and orbital has nothing to loop yet."),
    485: ("rejected", "Breadth", "'Wrapped' is an annual artefact for a site with two members; the taste data it would summarise does not exist."),
    486: ("rejected", "Breadth", "A printed poster is a second rendering pipeline for one annual artefact."),
    487: ("rejected", "Breadth", "A Bluesky feed generator is ATProto distribution; the estate's ATProto bridge is a darkhold row and orbital's federation target is ActivityPub."),
    489: ("rejected", "Breadth", "A native mobile app is a second client for a server-rendered site that works on a phone today."),
    494: ("rejected", "Breadth", "Collaborative taste vectors (couples, clubs) multiply the identity problem before one person's vector is used."),
    496: ("rejected", "Breadth", "Being a verification layer for the web is a positioning, not a feature, and it would publish review counts as a trust claim."),
    # contradicts the frictionless first contribution
    3:   ("rejected", "Frictionless", "Gating long reviews at TL1 is the friction this rebuild removes: the site starts open and closes one capability at a time."),
    308: ("rejected", "Frictionless", "Restricting tag creation to TL1+ blocks the first contribution from an anonymous visitor. Rate-limit and moderate instead."),
    414: ("rejected", "Frictionless", "Requiring a login to write is the whole thing the owner removed. Writing is open; abuse is bounded by rate limits and a moderation queue."),
    443: ("rejected", "Frictionless", "Invite codes assume an account is the unit of contribution, which it is not at the start."),
}

# ---- already true today, with the evidence ---------------------------------------
SHIPPED = {
    205: "src/prefs.rs::TunerView shows tuned/optional; the lens warm-up is 5 pages",
    211: "POST /prefs/reset (405 on GET), src/routes: /prefs/reset",
    317: "vertical tabs and tag filters are hrefs; src/render.rs Tab.href",
    333: "templates/base.html og:* blocks + generated static/og cards (orbital-metadata.timer)",
    334: "static/site.css is a single dark palette with no system-theme branch",
    335: "static/site.css 573 lines, no images, no webfonts; every page SSR",
    341: "templates/*.html are Askama; no catalogue route is client-rendered",
    346: "GET /api/v1/members sends a weak ETag and honours If-None-Match (304)",
    348: "every catalogue surface is a route; the SPA is confined to /app/",
    350: "/feed.xml 200 (commons-feed); per-vertical feeds do NOT exist yet",
    352: "templates/base.html skip link, first focusable element",
    353: "static/site.css section with prefers-reduced-motion",
    437: "templates/profile.html emits application/ld+json",
    438: "same: JSON-LD on the profile; entity pages inherit when they exist",
    463: "GET /healthz 200",
    461: "orbital-backup.timer + orbital-restore-test.timer (monthly restore)",
}

STATUS_TO_CF = {"draft": "validated", "shipped": "closed", "rejected": "rejected"}
EFFORT = lambda e: "S" if e <= 2 else "M" if e <= 4 else "L" if e <= 6 else "XL"
SEVERITY = lambda i: 5 if i >= 8 else 4 if i >= 6 else 3 if i >= 4 else 2 if i >= 3 else 1


def parse():
    text = open(SRC).read()
    text = text.split("## Prioritization Note")[0]
    roman, section = "?", "?"
    items = []
    for line in text.splitlines():
        m2 = re.match(r"^##\s+([IVX]+)\.", line)
        if m2:
            roman = m2.group(1)
            continue
        m3 = re.match(r"^###\s+([A-Z]+)\.", line)
        if m3:
            section = f"{roman}.{m3.group(1)}"
            continue
        m = re.match(r"^\d+\.\s+(.*)$", line)
        if m:
            n = len(items) + 1
            items.append((n, section, m.group(1).strip()))
    return items


def short_title(n, text, used):
    t = re.sub(r"\*\*|_", "", text)
    t = t.split(" — ")[0].split(": ")[0]
    if len(t) > 78:
        cut = t[:78]
        cut = cut[:cut.rfind(" ")] if " " in cut else cut
        t = cut.rstrip(",;") + "…"
    if t in used:
        t = f"{t} (#{n})"
    used[t] = n
    return t


def esc(s):
    return s.replace("'", "''")


def main():
    items = parse()
    assert len(items) == 496, f"parsed {len(items)} ideas, expected 496"
    used, rows = {}, []
    for n, section, text in items:
        verdict = "draft"
        reason = ""
        why = ""
        if n in REJECTED:
            verdict, why, reason = REJECTED[n]
        elif n in SHIPPED:
            verdict, reason = "shipped", SHIPPED[n]
        impact, effort = SECTION_SCORE.get(section, (5, 2))
        title = short_title(n, text, used)
        body = [f"Idea #{n} (§{section}) from the owner's 2026-11-10 orbital brainstorm.",
                "", text]
        if verdict == "shipped":
            body += ["", f"Shipped already, evidence: {reason}"]
        elif verdict == "rejected":
            body += ["", f"Refused — {why}: {reason}"]
        rows.append(dict(n=n, section=section, title=title, body="\n".join(body),
                         status=verdict, impact=impact, effort=effort,
                         reason=reason, text=text))

    out = []
    w = out.append
    w("-- Orbital: the 2026-11-10 brainstorm filed as complaint/feature pairs.")
    w("-- Generated from ~/secondbrain/10-Projects/orbital-ideas-2026-11-10.md (496 ideas).")
    w("-- Idempotent: every statement is guarded on (project slug, feature title).")
    w("-- Project resolved by SLUG inside each statement; ids are per-instance.")
    w("BEGIN;")
    w("")
    for r in rows:
        t = esc(r["title"])
        b = esc(r["body"])
        cf_title = esc("No " + r["title"][0].lower() + r["title"][1:])
        sev = SEVERITY(r["impact"])
        st = r["status"]
        cfs = STATUS_TO_CF[st]
        col = "done" if st == "shipped" else "rejected" if st == "rejected" else "inbox"
        w(f"-- #{r['n']} {r['section']} [{st}]")
        w(f"""INSERT INTO complaints (project_id, author_id, title, body, severity, frequency,
    strategic_multiplier, status, created_at, updated_at, origin)
SELECT p.id, 1, '{cf_title}',
  'Pain addressed by idea #{r['n']} (§{r['section']}). {esc(r['reason'])[:160]}',
  {sev}, 1.0, 1.0, '{cfs}', strftime('%s','now'), strftime('%s','now'), 'import'
FROM projects p WHERE p.slug = '{SLUG}'
  AND NOT EXISTS (SELECT 1 FROM complaints c WHERE c.project_id = p.id AND c.title = '{cf_title}');

INSERT INTO features (project_id, author_id, title, body, effort, status, strategic_weight,
    impact, effort_score, created_at, updated_at, origin)
SELECT p.id, 1, '{t}', '{b}', '{EFFORT(r['effort'])}', '{st}', 1.0,
  {r['impact']}, {r['effort']}, strftime('%s','now'), strftime('%s','now'), 'import'
FROM projects p WHERE p.slug = '{SLUG}'
  AND NOT EXISTS (SELECT 1 FROM features f WHERE f.project_id = p.id AND f.title = '{t}');

INSERT OR IGNORE INTO feature_complaints (feature_id, complaint_id)
SELECT f.id, c.id FROM features f JOIN projects p ON p.id = f.project_id
  JOIN complaints c ON c.project_id = p.id
WHERE p.slug = '{SLUG}' AND f.title = '{t}' AND c.title = '{cf_title}';

INSERT OR IGNORE INTO board_cards (project_id, kind, complaint_id, feature_id, column_id, entered_at)
SELECT p.id, 'feature', c.id, f.id, bc.id, strftime('%s','now')
FROM projects p
  JOIN features f ON f.project_id = p.id
  JOIN complaints c ON c.project_id = p.id
  JOIN board_columns bc ON bc.project_id = p.id AND bc.phase = '{col}'
WHERE p.slug = '{SLUG}' AND f.title = '{t}' AND c.title = '{cf_title}';

INSERT OR IGNORE INTO arena_entries (arena_id, entity_type, entity_id, r, rd, sigma, games, updated_at)
SELECT a.id, 'feature', f.id, 1500, 350, 0.06, 0, strftime('%s','now')
FROM arenas a JOIN projects p ON p.id = a.project_id
  JOIN features f ON f.project_id = p.id
WHERE p.slug = '{SLUG}' AND a.type = 'feature-priority' AND f.title = '{t}';
""")
    w(f"""INSERT INTO audit_log (project_id, actor_id, action, entity, entity_id, detail, created_at)
SELECT p.id, 1, 'import_ideas', 'project', p.id,
  'filed {len(rows)} ideas from the 2026-11-10 brainstorm ({sum(1 for r in rows if r['status']=='rejected')} refused with reasons)',
  strftime('%s','now')
FROM projects p WHERE p.slug = '{SLUG}';""")
    w("COMMIT;")
    open(OUT, "w").write("\n".join(out) + "\n")

    counts = collections.Counter(r["status"] for r in rows)
    by_sec = collections.Counter(r["section"] for r in rows)

    md = ["---", "title: Orbital brainstorm — disposition", "date: 2026-11-10",
          "project: orbital", "tags: [orbital, concord, brainstorm, disposition]", "---", "",
          "# Orbital — 2026-11-10 brainstorm, disposition",
          "",
          f"Source: `~/secondbrain/10-Projects/orbital-ideas-2026-11-10.md` ({len(rows)} ideas). "
          f"Filed into Concord project `orbital` on thinkcentre by "
          f"`~/code-local/rust/orbital/scripts/file-ideas.py`.",
          "",
          "| verdict | ideas |", "|---|---|"]
    for k in ("draft", "shipped", "rejected"):
        md.append(f"| {k} | {counts[k]} |")
    md += ["", "## Sections", "", "| section | ideas |", "|---|---|"]
    for s in sorted(by_sec, key=lambda x: (len(x.split('.')[0]), x)):
        md.append(f"| {s} | {by_sec[s]} |")
    md += ["", "## Refused, with the reason", "", "| # | verdict | reason |", "|---|---|---|"]
    for r in rows:
        if r["status"] == "rejected":
            md.append(f"| {r['n']} | {r['text'][:70]} | {r['reason'][:110]} |")
    md += ["", "## Already shipped", "", "| # | evidence |", "|---|---|"]
    for r in rows:
        if r["status"] == "shipped":
            md.append(f"| {r['n']} | {r['reason'][:120]} |")
    open(DISP, "w").write("\n".join(md) + "\n")
    print(f"sql {OUT}: {len(rows)} ideas {dict(counts)}")
    print(f"disposition {DISP}")


main()
