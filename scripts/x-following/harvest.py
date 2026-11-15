#!/usr/bin/env python3
"""Harvest the accounts behind a cookies.txt export can see: `following.jsonl`.

    ./harvest.py ~/Downloads/cookies.txt following.jsonl

Reads a Netscape-format cookie jar (browser export), takes `auth_token`, `ct0` and `twid` for
x.com and pages `1.1/friends/list` with the public web bearer. No API key, no developer account:
this is the same session a browser tab uses. 200 per page, 11 pages for 2,194 follows.

The response's `users[].url` is already the expanded bio URL and `description` is plain text —
t.co placeholders included, which `enrich.py` resolves.
"""
import json
import subprocess
import sys
import time

BEARER = ("AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhL"
          "TvJu4FA33AGWWjCpTnA")
UA = ("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) "
      "Chrome/131.0 Safari/537.36")


def cookies(path):
    jar = {}
    for line in open(path, encoding='utf-8'):
        line = line.rstrip('\n')
        if line.startswith('#HttpOnly_'):
            line = line[len('#HttpOnly_'):]
        elif line.startswith('#') or not line.strip():
            continue
        p = line.split('\t')
        if len(p) >= 7 and p[0].lstrip('.').endswith('x.com'):
            jar[p[5]] = p[6]
    for need in ('auth_token', 'ct0'):
        if need not in jar:
            sys.exit(f'cookies.txt has no x.com {need}: export while logged in')
    return jar


def main():
    jar = cookies(sys.argv[1])
    out = open(sys.argv[2] if len(sys.argv) > 2 else 'following.jsonl', 'w')
    header = '; '.join(f'{k}={v}' for k, v in jar.items())
    cursor, pages, seen = '-1', 0, set()
    while cursor and cursor != '0':
        url = ('https://api.x.com/1.1/friends/list.json?count=200&skip_status=true'
               f'&include_user_entities=false&cursor={cursor}')
        res = subprocess.run(
            ['curl', '-s', '-w', '\n%{http_code}', '-H', f'cookie: {header}',
             '-H', f'authorization: Bearer {BEARER}',
             '-H', f"x-csrf-token: {jar['ct0']}",
             '-H', 'x-twitter-auth-type: OAuth2Session',
             '-H', 'x-twitter-active-user: yes', '-A', UA, url],
            capture_output=True, text=True)
        body, _, code = res.stdout.rpartition('\n')
        if code != '200':
            sys.exit(f'page {pages + 1}: HTTP {code} {body[:200]}')
        data = json.loads(body)
        for u in data.get('users', []):
            if u['id_str'] not in seen:
                seen.add(u['id_str'])
                out.write(json.dumps(u) + '\n')
        pages += 1
        cursor = data.get('next_cursor_str')
        print(f'page {pages}: {len(seen)} follows', flush=True)
        time.sleep(2)
    out.close()
    print(f'{len(seen)} follows -> {out.name}')


if __name__ == '__main__':
    main()
