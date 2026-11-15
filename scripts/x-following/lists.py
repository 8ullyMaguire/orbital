#!/usr/bin/env python3
"""Harvest the members of a curator's public X lists: `following.jsonl`, same shape.

    ./lists.py ~/Downloads/cookies.txt Breeder592 following.jsonl

STATUS 2026-11-15: does not work. `1.1/lists/members` answers 404 "that page does not
exist" from both api.x.com and x.com/i/api, while `1.1/lists/list` still answers — X retired
the members endpoint and the replacement is a GraphQL operation whose query id lives in the
logged-in app bundle. The file is kept as the record of that finding, not as a working tool.

Why lists: a following list is one person's taste; a public list is a *named* slice of it
("@CuckPorn2/bbc", "Erotica Writers", "Sissies / Femboys" — 58 lists, 16,407 memberships
on the operator's account). The members are ordinary user objects, so everything
downstream — `enrich.py`, `build.py`, `import.sql` — works unchanged.

Same auth as `harvest.py`: `auth_token`, `ct0`, `twid` from a browser cookie jar and the
public web bearer, no developer account. `lists/members` pages 100 at a time; a list that
fails does not abort the run, it is reported and the next one starts.
"""
import json
import re
import subprocess
import sys
import time

from harvest import BEARER, UA, cookies

API = 'https://api.x.com/1.1'


def get(jar, path, params=''):
    header = '; '.join(f'{k}={v}' for k, v in jar.items())
    url = f'{API}/{path}{params}'
    for attempt in range(3):
        res = subprocess.run(
            ['curl', '-s', '-w', '\n%{http_code}', '-H', f'cookie: {header}',
             '-H', f'authorization: Bearer {BEARER}',
             '-H', f"x-csrf-token: {jar['ct0']}",
             '-H', 'x-twitter-auth-type: OAuth2Session',
             '-H', 'x-twitter-active-user: yes', '-A', UA, url],
            capture_output=True, text=True)
        body, _, code = res.stdout.rpartition('\n')
        if code == '200':
            return json.loads(body)
        if code == '429':
            time.sleep(15 * (attempt + 1))
            continue
        raise RuntimeError(f'HTTP {code} {body[:160]}')
    raise RuntimeError('rate limited after three attempts')


def main():
    jar = cookies(sys.argv[1])
    who = sys.argv[2]
    out_path = sys.argv[3] if len(sys.argv) > 3 else 'following.jsonl'

    lists = get(jar, 'lists/list.json', f'?screen_name={who}')
    print(f'{len(lists)} lists by @{who}, '
          f'{sum(l["member_count"] for l in lists)} memberships (overlapping)', flush=True)

    seen, wrote = set(), 0
    with open(out_path, 'w') as out:
        for i, lst in enumerate(lists, 1):
            cursor, page_members = '-1', 0
            try:
                while cursor and cursor != '0':
                    data = get(jar, 'lists/members.json',
                               f'?list_id={lst["id_str"]}&count=100&skip_status=true'
                               f'&include_user_entities=false&cursor={cursor}')
                    for u in data.get('users', []):
                        if u['id_str'] in seen:
                            continue
                        seen.add(u['id_str'])
                        out.write(json.dumps(u) + '\n')
                        wrote += 1
                    page_members += len(data.get('users', []))
                    cursor = data.get('next_cursor_str')
                    time.sleep(1.2)
            except Exception as e:
                print(f'  list {i} "{lst["name"][:32]}" failed: {e}', flush=True)
                continue
            out.flush()
            print(f'  {i}/{len(lists)} {lst["name"][:32]:32} {page_members:>5} members '
                  f'({wrote} unique so far)', flush=True)
    print(f'{wrote} unique accounts -> {out_path}', flush=True)


if __name__ == '__main__':
    main()
