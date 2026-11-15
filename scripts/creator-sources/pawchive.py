#!/usr/bin/env python3
"""Walk Pawchive's "artists similar to" graph, one Patreon creator at a time.

    ./pawchive.py --seed 152786190 --depth 2 --max 5000 --out pawchive.jsonl

In : a seed Patreon user id (or several, --seed comma separated)
Out: one JSON line per creator: {id, name, url}

Why BFS: Pawchive is a Patreon archive whose `/patreon/user/<id>/recommended` page lists about
fifty artists similar to that one, with their display names. Following those edges from one seed
is a cheap, self-organising catalogue of people who publish — each node is a real page with a
name, not an inference from a mention.

What it deliberately does not do: it does not fetch posts, media or the creator's own page tree.
The creator page carries a display name and the Patreon URL and nothing else worth storing, so one
request per node is the whole cost.

Politeness: one request per second per worker, a browser UA, and a `--max` ceiling. A page that
fails is skipped and can be re-walked later; the output file is appendable and deduped on read.
"""
import argparse
import collections
import json
import os
import re
import subprocess
import sys
import time

UA = ('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 '
      '(KHTML, like Gecko) Chrome/131.0 Safari/537.36')
BASE = 'https://pawchive.pw'
USER = re.compile(r'/patreon/user/(\d+)')
TITLE = re.compile(r'<title>(.*?)</title>', re.S)


def get(url, timeout=30):
    for attempt in range(3):
        r = subprocess.run(['curl', '-sL', '--max-time', str(timeout), '-A', UA, url],
                           capture_output=True, text=True, timeout=timeout + 10)
        if r.stdout and 'Just a moment' not in r.stdout[:4000]:
            return r.stdout
        time.sleep(2 * (attempt + 1))
    return ''


def similar(uid):
    """The ids this creator's recommended page points at."""
    body = get(f'{BASE}/patreon/user/{uid}/recommended')
    if not body:
        return []
    return [x for x in dict.fromkeys(USER.findall(body)) if x != uid]


def name_of(uid):
    body = get(f'{BASE}/patreon/user/{uid}')
    if not body:
        return ''
    m = TITLE.search(body)
    if not m:
        return ''
    title = re.sub(r'\s*\|.*$', '', m.group(1)).strip()
    title = re.sub(r'^Posts of\s+', '', title)
    title = re.sub(r'\s+from Patreon$', '', title)
    return re.sub(r'&amp;', '&', title).strip()[:80]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--seed', required=True)
    ap.add_argument('--depth', type=int, default=2)
    ap.add_argument('--max', type=int, default=5000)
    ap.add_argument('--out', default=os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                                  'pawchive.jsonl'))
    ap.add_argument('--pause', type=float, default=1.0)
    args = ap.parse_args()

    known = {}
    if os.path.exists(args.out):
        for line in open(args.out):
            try:
                r = json.loads(line)
            except Exception:
                continue
            known[r['id']] = r

    queue = collections.deque((uid, 0) for uid in args.seed.split(','))
    seen = set(known) | {u for u, _ in queue}
    fh = open(args.out, 'a')
    added = 0
    while queue and len(known) < args.max:
        uid, depth = queue.popleft()
        if uid not in known:
            name = name_of(uid)
            rec = {'id': uid, 'name': name,
                   'url': f'https://www.patreon.com/user?u={uid}'}
            known[uid] = rec
            fh.write(json.dumps(rec) + '\n')
            fh.flush()
            added += 1
            if len(known) % 25 == 0:
                print(f'  {len(known)} creators, {len(queue)} queued, depth {depth}', flush=True)
        if depth >= args.depth:
            time.sleep(args.pause)
            continue
        for nxt in similar(uid):
            if nxt not in seen:
                seen.add(nxt)
                queue.append((nxt, depth + 1))
        time.sleep(args.pause)
    fh.close()
    print(f'creators: {len(known)} (added {added}), queue left {len(queue)} -> {args.out}', flush=True)
    if len(known) < args.max and not queue:
        print('note: the graph was exhausted before --max', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
