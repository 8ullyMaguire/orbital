#!/usr/bin/env python3
"""The periodic creator crawl: stash-box → Orbital, incrementally.

    ./crawl.py                 # what the timer runs: everything updated since the cursor
    ./crawl.py --full          # the first run: sweep every performer on both instances
    ./crawl.py --dry-run       # build and report, write nothing to the database

State is a JSON cursor per instance (`~/.local/state/orbital-creators.json`) holding the newest
`updated` stamp seen. The sweep walks newest-updated first and stops at the cursor, so a weekly run
is a few pages, not the whole table.

Why a cursor rather than "everything again": the best answer to "did this already land" is the
identity check in build.py (an X handle or a creator URL that Orbital already has), and the cursor
only decides how much of the source to read. Both must be present — the cursor alone would miss a
performer whose record did not change while their socials did appear elsewhere.
"""
import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
STATE = os.path.expanduser('~/.local/state/orbital-creators.json')
SITES = {'stashdb': 'https://stashdb.org/graphql', 'fansdb': 'https://fansdb.cc/graphql'}


def sh(cmd, **kw):
    print('+', ' '.join(cmd[:6]), '…' if len(cmd) > 6 else '', flush=True)
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def db_url():
    url = os.environ.get('DATABASE_URL')
    if url:
        return url
    env = os.path.expanduser('~/.config/systemd/user/orbital-db.env')
    if os.path.exists(env):
        for line in open(env):
            if line.startswith('DATABASE_URL='):
                return line.split('=', 1)[1].strip()
    sys.exit('DATABASE_URL is not set and no orbital-db.env was found')


def load_state():
    if os.path.exists(STATE):
        return json.load(open(STATE))
    return {s: '' for s in SITES}


def save_state(state):
    os.makedirs(os.path.dirname(STATE), exist_ok=True)
    json.dump(state, open(STATE, 'w'), indent=1)


def export_identities(url, path):
    q = ("select u.handle || '|' || coalesce(l.url, '') from users u "
         "left join links l on l.user_id = u.id")
    with open(path, 'w') as f:
        r = subprocess.run(['psql', url, '-tAc', q], stdout=f, stderr=subprocess.PIPE, text=True)
    if r.returncode != 0:
        sys.exit(f'psql failed: {r.stderr[:300]}')
    print(f'identities: {sum(1 for _ in open(path))} rows -> {path}', flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--full', action='store_true')
    ap.add_argument('--dry-run', action='store_true')
    ap.add_argument('--max-pages', type=int, default=60,
                    help='per instance and per run: a ceiling on the incremental sweep')
    ap.add_argument('--sites', default='stashdb,fansdb')
    ap.add_argument('--env-file', default=os.path.expanduser('~/.config/orbital-creators.env'),
                    help='holds STASHDB_KEY / FANSDB_KEY; kept out of the repo on purpose')
    args = ap.parse_args()

    if not os.path.exists(args.env_file):
        sys.exit(f'{args.env_file} is missing: it holds STASHDB_KEY / FANSDB_KEY and is not in the repo')
    state = load_state()
    url = db_url()
    ident = os.path.join(HERE, 'orbital-ident.txt')
    export_identities(url, ident)

    since = {s: '' for s in SITES} if args.full else state
    inputs = []
    for site in args.sites.split(','):
        out = os.path.join(HERE, f'new_{site}.jsonl')
        cmd = [sys.executable, os.path.join(HERE, 'sweep.py'), '--site', site,
               '--out', out, '--workers', '8', '--sort', 'UPDATED_AT',
               '--max-pages', str(args.max_pages), '--env-file', args.env_file]
        if since.get(site):
            cmd += ['--since', since[site]]
        r = sh(cmd)
        print((r.stdout or '').strip().splitlines()[-1] if r.stdout else r.stderr[-300:], flush=True)
        if r.returncode != 0 or not os.path.exists(out):
            print(f'  {site}: sweep failed, keeping the previous cursor', flush=True)
            continue
        newest = ''
        for line in open(out):
            try:
                p = json.loads(line)
            except Exception:
                continue
            newest = max(newest, p.get('updated') or '')
        inputs.append(f'{site}:{out}')
        if newest and not args.dry_run:
            state[site] = newest

    if not inputs:
        sys.exit('nothing swept')

    stage = os.path.join(HERE, 'stage-creators.csv')
    r = sh([sys.executable, os.path.join(HERE, 'build.py'), '--stage', stage,
            '--records', os.path.join(HERE, 'creators.json'), '--ident', ident,
            '--inputs', ','.join(inputs)])
    print(r.stdout, flush=True)
    if r.returncode != 0:
        sys.exit(f'build failed: {r.stderr[-500:]}')

    if args.dry_run:
        print('dry run: nothing written', flush=True)
        return
    sql = open(os.path.join(HERE, 'import.sql')).read().replace('__STAGE__', stage)
    run_sql = f'/tmp/orbital-import-{os.getpid()}.sql'
    open(run_sql, 'w').write(sql)
    r = sh(['psql', url, '-f', run_sql])
    os.unlink(run_sql)
    tail = [l for l in (r.stdout or '').splitlines() if l.startswith(('INSERT', 'COMMIT', 'ERROR'))]
    print('\n'.join(tail), flush=True)
    if r.returncode != 0:
        sys.exit(f'import failed: {r.stderr[-500:]}')
    save_state({k: v for k, v in state.items() if v})
    print('cursor:', json.dumps(state), flush=True)


if __name__ == '__main__':
    main()
