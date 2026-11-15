#!/usr/bin/env python3
"""Sweep a whole stash-box performer table in parallel.

    ./sweep.py --site stashdb --out stashdb_all.jsonl --workers 8
    ./sweep.py --site fansdb  --out fansdb_all.jsonl  --workers 6

Pages are independent, so they are fetched by a thread pool and appended as they
arrive; a later `dedupe` step keys on the performer id. `--since` performs the
incremental crawl instead: it walks newest-updated first and stops at the cursor,
which is what the periodic timer runs.
"""
import argparse
import json
import os
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor

UA = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36'
FIELDS = ('id name disambiguation aliases gender birth_date country ethnicity '
          'career_start_year career_end_year scene_count created updated '
          'urls { url site { name } } images { url }')
QUERY = '{ queryPerformers(input:{per_page:100,page:%d,sort:%s,direction:DESC}) { count performers {%s} } }'
URLS = {'stashdb': 'https://stashdb.org/graphql', 'fansdb': 'https://fansdb.cc/graphql',
        'theporndb': 'https://theporndb.net/graphql'}
KEYS = {'stashdb': 'STASHDB_KEY', 'fansdb': 'FANSDB_KEY', 'theporndb': 'STASH_APP_TOKEN'}

lock = threading.Lock()


def env(env_file):
    out = {}
    for line in open(os.path.expanduser(env_file)):
        if '=' in line:
            k, v = line.strip().split('=', 1)
            out[k] = v
    return out


def page(url, key, header, n, sort):
    for attempt in range(4):
        r = subprocess.run(['curl', '-s', '--max-time', '90', '-X', 'POST', url,
                            '-H', 'Content-Type: application/json', '-H', f'{header}: {key}',
                            '-d', json.dumps({'query': QUERY % (n, sort, FIELDS)}), '-A', UA],
                           capture_output=True, text=True)
        try:
            d = json.loads(r.stdout)
        except Exception:
            d = {}
        if d.get('data', {}) and d['data'].get('queryPerformers'):
            return d['data']['queryPerformers']
        if attempt == 3:
            return {'error': (d.get('errors') or r.stdout[:200])}
        time.sleep(1.5 * (attempt + 1))
    return {'error': 'unreachable'}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--site', default='stashdb')
    ap.add_argument('--out', required=True)
    ap.add_argument('--workers', type=int, default=8)
    ap.add_argument('--sort', default='CREATED_AT')
    ap.add_argument('--since', default='')
    ap.add_argument('--max-pages', type=int, default=0)
    ap.add_argument('--start-page', type=int, default=1)
    ap.add_argument('--append', action='store_true')
    ap.add_argument('--env-file', default=os.path.join(os.path.dirname(os.path.abspath(__file__)), 'api.env'))
    args = ap.parse_args()

    e = env(args.env_file)
    key = e[KEYS[args.site]]
    url = URLS[args.site]
    header = 'ApiKey'

    first = page(url, key, header, 1, args.sort)
    if 'error' in first:
        sys.exit(f'first page failed: {first["error"]}')
    total = first['count']
    last = (total + 99) // 100
    first_page = max(1, args.start_page)
    pages = last
    if args.max_pages:
        pages = min(pages, first_page + args.max_pages - 1)
    print(f'{args.site}: {total} performers, {pages} pages, {args.workers} workers', flush=True)

    fh = open(args.out, 'a' if args.append else 'w')
    seen = [0]
    stop = threading.Event()

    def emit(rows, n):
        with lock:
            for p in rows:
                if args.since and (p.get('updated') or '') < args.since:
                    stop.set()
                    continue
                fh.write(json.dumps(p) + '\n')
                seen[0] += 1
            if n % 25 == 0:
                fh.flush()
                print(f'  {n}/{pages} pages, {seen[0]} performers', flush=True)

    if first_page == 1:
        emit(first['performers'], 1)
    with ThreadPoolExecutor(max_workers=args.workers) as ex:
        futs = {}
        for n in range(max(2, first_page), pages + 1):
            futs[ex.submit(page, url, key, header, n, args.sort)] = n
        for done, fut in enumerate(futs, start=max(2, first_page)):
            res = fut.result()
            if 'error' in res:
                print(f'  page failed: {res["error"]}', flush=True)
                continue
            emit(res['performers'], done)
            if stop.is_set():
                for f in futs:
                    f.cancel()
                break
    fh.close()
    print(f'{seen[0]} performers -> {args.out}', flush=True)


if __name__ == '__main__':
    main()
