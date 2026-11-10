#!/usr/bin/env python3
"""Generate the SQL that files the 2026-11-10 "fame headwind" brainstorm into Concord project 'orbital'.

Source: ~/secondbrain/10-Projects/orbital-fame-headwind-2026-11-10.md (20 numbered ideas).
Output: <scratch>/orbital-fame-filing.sql — idempotent, guarded on (project slug, feature title).

Same shape as `file-ideas.py`, because the page reads one shape: complaint -> feature ->
feature_complaints -> board_cards -> arena_entries. Titles carry a `Fame headwind: ` prefix so this
batch can never collide with the 496-idea import, and re-running it inserts nothing.

Jobs:
    python3 scripts/file-fame-ideas.py            # write the SQL
    python3 scripts/file-fame-ideas.py --check    # write it and print the plan
"""
import argparse
import collections
import os
import re

SRC = os.path.expanduser("~/secondbrain/10-Projects/orbital-fame-headwind-2026-11-10.md")
OUT = os.path.expanduser(
    "~/.hermes/profiles/coding/cache/scratch/orbital-fame-filing.sql"
)
SLUG = "orbital"
PREFIX = "Fame headwind: "

# The owner's own disposition, recorded on the page so a later session does not re-litigate it.
REJECTED = {
    14: ("Search", "Damped search order mixes two different promises: relevance and visibility. A zero-result "
                   "query would become indistinguishable from a suppressed one. Search keeps its own golden set "
                   "(roadmap step 22) and may adopt damping only after that exists."),
}

# Ideas that the roadmap picks up now, with the step they became. Recorded because "filed" and
# "scheduled" are different facts and the page should carry both.
ROADMAP = {
    15: "Concentration baseline (Phase A, new step)",
    1: "Attention weighting (Phase B)",
    2: "Attention weighting (Phase B)",
    5: "Attention weighting (Phase B)",
    3: "Amortised fame (Phase B)",
    6: "Exposure caps (Phase B)",
    7: "Exposure caps (Phase B)",
    10: "Exposure caps (Phase B)",
    8: "Impression ceiling and rotation (Phase B)",
    9: "Impression ceiling and rotation (Phase B)",
    11: "Undiscovered shelf (Phase B)",
    4: "Undiscovered shelf (Phase B)",
    16: "Guardrail and policy sentence (Phase B)",
    17: "Guardrail and policy sentence (Phase B)",
    18: "Guardrail and policy sentence (Phase B)",
}

# (impact 1-10, effort weeks 1-10)
SCORE = {
    1: (9, 1), 2: (9, 4), 3: (7, 2), 4: (7, 2), 5: (6, 1),
    6: (8, 1), 7: (8, 1), 8: (6, 2), 9: (6, 1), 10: (5, 1),
    11: (8, 2), 12: (6, 3), 13: (7, 1), 14: (4, 2),
    15: (9, 2), 16: (8, 1), 17: (7, 1), 18: (5, 1), 19: (6, 2), 20: (6, 1),
}

STATUS_TO_CF = {"draft": "validated", "shipped": "closed", "rejected": "rejected"}
EFFORT = lambda e: "S" if e <= 2 else "M" if e <= 4 else "L" if e <= 6 else "XL"
SEVERITY = lambda i: 5 if i >= 8 else 4 if i >= 6 else 3 if i >= 4 else 2 if i >= 3 else 1


def parse():
    text = open(SRC).read()
    # Only the idea list: stop at the disposition table so its numbered references are not parsed.
    body = text.split("## Disposition")[0]
    section = "?"
    items = []
    for line in body.splitlines():
        m = re.match(r"^##\s+([A-D])\.\s+(.*)$", line)
        if m:
            section = m.group(1)
            continue
        m = re.match(r"^(\d+)\.\s+(.*)$", line)
        if m:
            n = int(m.group(1))
            assert n == len(items) + 1, f"idea numbering is not contiguous at {n}"
            items.append((n, section, m.group(2).strip()))
    return items


def short_title(n, text):
    t = re.sub(r"\*\*|_", "", text)
    t = t.split(" — ")[0].split(". ")[0]
    if len(t) > 70:
        cut = t[:70]
        cut = cut[: cut.rfind(" ")] if " " in cut else cut
        t = cut.rstrip(",;") + "…"
    return PREFIX + t


def esc(s):
    return s.replace("'", "''")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    items = parse()
    assert len(items) == 20, f"parsed {len(items)} ideas, expected 20"
    rows = []
    for n, section, text in items:
        impact, effort = SCORE[n]
        status, reason = ("rejected", REJECTED[n][1]) if n in REJECTED else ("draft", "")
        title = short_title(n, text)
        body = [f"Idea #{n} (§{section}) from the 2026-11-10 fame-headwind brainstorm.", "", text]
        if n in REJECTED:
            body += ["", f"Refused — {REJECTED[n][0]}: {reason}"]
        elif n in ROADMAP:
            body += ["", f"Scheduled: {ROADMAP[n]}."]
        rows.append(dict(n=n, title=title, body="\n".join(body), status=status,
                         impact=impact, effort=effort))

    out = ["-- Orbital: the 2026-11-10 fame-headwind brainstorm filed as complaint/feature pairs.",
           "-- Generated from ~/secondbrain/10-Projects/orbital-fame-headwind-2026-11-10.md.",
           "-- Idempotent: guarded on (project slug, title). Project resolved by SLUG inside each statement.",
           "BEGIN;", ""]
    w = out.append
    for r in rows:
        t = esc(r["title"])
        b = esc(r["body"])
        cf = esc("No " + r["title"][0].lower() + r["title"][1:])
        cfs = STATUS_TO_CF[r["status"]]
        col = "done" if r["status"] == "shipped" else "rejected" if r["status"] == "rejected" else "inbox"
        w(f"-- #{r['n']} [{r['status']}] {r['title']}")
        w(f"""INSERT INTO complaints (project_id, author_id, title, body, severity, frequency,
    strategic_multiplier, status, created_at, updated_at, origin)
SELECT p.id, 1, '{cf}',
  'Concentration pain addressed by idea #{r['n']}.',
  {SEVERITY(r['impact'])}, 1.0, 1.0, '{cfs}', strftime('%s','now'), strftime('%s','now'), 'import'
FROM projects p WHERE p.slug = '{SLUG}'
  AND NOT EXISTS (SELECT 1 FROM complaints c WHERE c.project_id = p.id AND c.title = '{cf}');

INSERT INTO features (project_id, author_id, title, body, effort, status, strategic_weight,
    impact, effort_score, created_at, updated_at, origin)
SELECT p.id, 1, '{t}', '{b}', '{EFFORT(r['effort'])}', '{r['status']}', 1.0,
  {r['impact']}, {r['effort']}, strftime('%s','now'), strftime('%s','now'), 'import'
FROM projects p WHERE p.slug = '{SLUG}'
  AND NOT EXISTS (SELECT 1 FROM features f WHERE f.project_id = p.id AND f.title = '{t}');

INSERT OR IGNORE INTO feature_complaints (feature_id, complaint_id)
SELECT f.id, c.id FROM features f JOIN projects p ON p.id = f.project_id
  JOIN complaints c ON c.project_id = p.id
WHERE p.slug = '{SLUG}' AND f.title = '{t}' AND c.title = '{cf}';

INSERT OR IGNORE INTO board_cards (project_id, kind, complaint_id, feature_id, column_id, entered_at)
SELECT p.id, 'feature', c.id, f.id, bc.id, strftime('%s','now')
FROM projects p
  JOIN features f ON f.project_id = p.id
  JOIN complaints c ON c.project_id = p.id
  JOIN board_columns bc ON bc.project_id = p.id AND bc.phase = '{col}'
WHERE p.slug = '{SLUG}' AND f.title = '{t}' AND c.title = '{cf}';

INSERT OR IGNORE INTO arena_entries (arena_id, entity_type, entity_id, r, rd, sigma, games, updated_at)
SELECT a.id, 'feature', f.id, 1500, 350, 0.06, 0, strftime('%s','now')
FROM arenas a JOIN projects p ON p.id = a.project_id
  JOIN features f ON f.project_id = p.id
WHERE p.slug = '{SLUG}' AND a.type = 'feature-priority' AND f.title = '{t}';
""")
    w(f"""INSERT INTO audit_log (project_id, actor_id, action, entity, entity_id, detail, created_at)
SELECT p.id, 1, 'import_ideas', 'project', p.id,
  'filed {len(rows)} fame-headwind ideas ({sum(1 for r in rows if r['status'] == 'rejected')} refused with reasons, '
  || '{sum(1 for r in rows if r['n'] in ROADMAP)} scheduled on the roadmap)',
  strftime('%s','now')
FROM projects p WHERE p.slug = '{SLUG}';""")
    w("COMMIT;")
    open(OUT, "w").write("\n".join(out) + "\n")

    counts = collections.Counter(r["status"] for r in rows)
    print(f"sql {OUT}: {len(rows)} ideas {dict(counts)}")
    print(f"scheduled: {len(ROADMAP)} of 20 → roadmap")
    if args.check:
        for r in rows:
            print(f"  #{r['n']:2d} [{r['status']:8s}] {r['title']}")


main()
