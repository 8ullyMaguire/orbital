#!/usr/bin/env python3
"""Expand the link-in-bio pages the registry rows point at.

    ./enrich.py --stage stage-creators.csv --out enrich.json
    ./enrich.py --stage stage-creators.csv --out enrich.json --limit 20   # a probe

In : stage-creators.csv (one row per imported creator; links JSON in the last column)
Out: enrich.json  {"https://linktr.ee/<x>": ["https://onlyfans.com/<x>", ...], ...}

Why this exists. A stash-box record carries the URLs someone entered, and a large
share of them are aggregator pages: 13,784 of the 44,817 imported creators point at
linktr.ee, allmylinks, beacons or link.me, and the socials live one hop further out.
The X-following import expanded 303 such pages; this is the same step at registry
scale, and it is resumable — a URL already in `enrich.json` is not fetched again.

Only URLs that `build.classify()` recognises as a creator surface are kept. A page's
own chrome (its CDN, its analytics, the other members it recommends) is exactly the
kind of link that would make a directory of strangers.

The output is merged by `merge.py`, which inserts what is missing and re-runs the
same idempotent SQL shape as `import.sql`.
"""
import argparse
import concurrent.futures as cf
import csv
import json
import os
import re
import subprocess
import sys
import urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from build import classify  # noqa: E402  one definition of "a creator surface"

UA = ('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 '
      '(KHTML, like Gecko) Chrome/131.0 Safari/537.36')

# Hosts whose page is a list of somebody's elsewhere. `classify()` calls these
# `linkinbio`; the list is repeated here only to decide what is worth fetching.
AGG = ('linktr.ee', 'allmylinks.com', 'beacons.ai', 'beacons.page', 'link.me',
       'hoo.be', 'vxn.link', 'youpay.bio', 'sent.bio', 'oopsie.bio', 'bio.site',
       'linkr.bio', 'carrd.co', 'milkshake.app', 'komi.io', 'lnk.bio', 'solo.to',
       'bio.link', 'taplink.cc', 'linkin.bio', 'campsite.bio', 'many.link',
       'snap.bio', 'getallmylinks.com', 'snipfeed.co', 'stan.store')

# The page's own furniture. A link here is not the creator's.
JUNK = ('linktr.ee', 'allmylinks.com', 'beacons.ai', 'link.me', 'w3.org', 'schema.org',
        'gstatic', 'googleapis', 'google-analytics', 'googletagmanager', 'doubleclick',
        'cloudflare', 'cloudfront', 'amazonaws', 'sentry', 'imgix', 'unsplash',
        'mozilla.org', 'apple.com', 'twitter.com/i/', 'x.com/i/', 'instagram.com/p/',
        'facebook.com/tr', 'fonts.', 'discord.com/assets', 'shopify.com', 'cdn.shopify',
        'youtube.com/embed', 'clarity.ms', 'cookielaw', 'onetrust')


def host_of(u):
    m = re.match(r'https?://([^/]+)', u or '')
    return m.group(1).lower().replace('www.', '') if m else ''


def is_aggregator(u):
    h = host_of(u)
    return any(h == a or h.endswith('.' + a) for a in AGG)


def fetch(url, timeout=25):
    try:
        r = subprocess.run(
            ['curl', '-sL', '--max-time', str(timeout), '-A', UA,
             '-H', 'Accept-Language: en-US,en;q=0.9', url],
            capture_output=True, text=True, timeout=timeout + 10)
        return r.stdout
    except Exception:
        return ''


def extract(url):
    """Creator surfaces the page links to, as (kind, url) pairs, deduped by identity."""
    body = fetch(url)
    if not body:
        return None
    out, seen = [], set()
    for raw in re.findall(r'https?://[A-Za-z0-9._%+\-]+/[A-Za-z0-9._%+\-/?#=&~]*', body):
        candidate = raw.rstrip('.,);"\'')
        low = candidate.lower()
        if any(j in low for j in JUNK):
            continue
        c = classify(candidate)
        if not c:
            continue
        kind, _label, _icon, key = c
        if kind in ('linkinbio', 'website') or not key:
            # An unknown host is a guess, and a bare host is not a profile; a page's
            # body is full of both (a wishlist, a consent script, `tiktok.com/`).
            continue
        # Normalise the query string away so one profile is one row.
        clean = urllib.parse.urlsplit(candidate)
        clean = urllib.parse.urlunsplit((clean.scheme, clean.netloc, clean.path, '', ''))
        ident = key or clean
        if ident in seen:
            continue
        seen.add(ident)
        out.append({'url': clean, 'label': _label, 'icon': _icon})
    return out


def aggregator_urls(stage):
    urls = {}
    with open(stage, newline='') as f:
        for row in csv.reader(f):
            if not row or len(row) < 8:
                continue
            handle, links = row[0], json.loads(row[7] or '[]')
            for l in links:
                u = (l.get('url') or '').strip()
                if is_aggregator(u):
                    urls.setdefault(u, set()).add(handle)
    return urls


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--stage', default=os.path.join(HERE, 'stage-creators.csv'))
    ap.add_argument('--out', default=os.path.join(HERE, 'enrich.json'))
    ap.add_argument('--workers', type=int, default=6)
    ap.add_argument('--limit', type=int, default=0)
    ap.add_argument('--only', default='', help='comma-separated hosts to restrict to')
    args = ap.parse_args()

    agg = aggregator_urls(args.stage)
    if args.only:
        want = set(args.only.split(','))
        agg = {u: h for u, h in agg.items() if host_of(u) in want}
    data = json.load(open(args.out)) if os.path.exists(args.out) else {}
    todo = [u for u in agg if u not in data]
    if args.limit:
        todo = todo[:args.limit]
    print(f'aggregator urls: {len(agg)} across {len({host_of(u) for u in agg})} hosts; '
          f'{len(todo)} to fetch', flush=True)

    done = 0
    with cf.ThreadPoolExecutor(max_workers=args.workers) as ex:
        futs = {ex.submit(extract, u): u for u in todo}
        for fut in cf.as_completed(futs):
            u = futs[fut]
            try:
                data[u] = fut.result()
            except Exception as e:  # a page that cannot be read is recorded, not retried forever
                data[u] = None
                print('ERR', u, e, flush=True)
            done += 1
            if done % 50 == 0:
                json.dump(data, open(args.out, 'w'))
                ok = sum(1 for v in data.values() if v)
                print(f'  {done}/{len(todo)} fetched, {ok} pages carried a creator link',
                      flush=True)
    json.dump(data, open(args.out, 'w'))
    ok = sum(1 for v in data.values() if v)
    total = sum(len(v) for v in data.values() if v)
    print(f'done: {len(data)} urls, {ok} with links, {total} creator links found', flush=True)


if __name__ == '__main__':
    main()
