#!/usr/bin/env python3
"""Expand link-in-bio aggregators + resolve t.co for the X following harvest.

In:  following.jsonl  (v1.1 friends/list users)
Out: enrich.json  {screen_name: {"extra_links": [...], "tco": {...}}}
"""
import json, re, subprocess, sys, concurrent.futures as cf, os

UA = ('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 '
      '(KHTML, like Gecko) Chrome/131.0 Safari/537.36')
HERE = os.path.dirname(os.path.abspath(__file__))

JUNK = ('linktr.ee', 'ugc.production', 'assets.production', 'allmylinks.com',
        'allmylinks.help', 'beacons.ai', 'beacons.page', 'link.me', 'hoo.be',
        'carrd.co', 'vxn.link', 'youpay.bio', 'sent.bio', 'oopsie.bio',
        'bio.site', 'linkr.bio', 'consentjs.datagrail', 'gstatic', 'w3.org',
        'schema.org', 'cloudflare', 'fonts.', 'imgix', 'amazonaws', 'sentry',
        'facebook.com/tr', 'twitter.com/i/', 'apple.com', 'mozilla.org',
        'youtube.com/embed', 'unsplash.com', 'coverr.co', 'clarity.ms',
        'doubleclick', 'google-analytics', 'googletagmanager', 't.co/',
        'linktr.ee/', 'discord.com/assets', 'shopify.com', 'cdn.shopify',
        'instagram.com/$', 'twitter.com/$', 'x.com/$', 'discord.gg/$')

AGG_HOSTS = ('linktr.ee', 'allmylinks.com', 'beacons.ai', 'beacons.page',
             'link.me', 'hoo.be', 'vxn.link', 'youpay.bio', 'sent.bio',
             'oopsie.bio', 'bio.site', 'linkr.bio', 'carrd.co', 'milkshake.app',
             'komi.io', 'lnk.bio', 'solo.to', 'bio.link', 'taplink.cc',
             'allmylinks.help', 'linkin.bio', 'campsite.bio', 'many.link',
             'linkr.bio', 'snap.bio', 'bit.ly')

SOCIAL_HOSTS = {
    'instagram.com': 'instagram', 'instagr.am': 'instagram',
    'tiktok.com': 'tiktok', 'twitter.com': 'x', 'x.com': 'x',
    'bsky.app': 'bsky', 'youtube.com': 'youtube', 'youtu.be': 'youtube',
    'twitch.tv': 'twitch', 'fansly.com': 'fansly', 'onlyfans.com': 'onlyfans',
    'patreon.com': 'patreon', 'discord.gg': 'discord', 'discord.com': 'discord',
    'reddit.com': 'reddit', 'tumblr.com': 'tumblr', 'pixiv.net': 'pixiv',
    'ko-fi.com': 'kofi', 'throne.com': 'throne', 'throne.com/': 'throne',
    'manyvids.com': 'manyvids', 'fetlife.com': 'fetlife',
    'snapchat.com': 'snapchat', 't.me': 'telegram', 'telegram.me': 'telegram',
    'spotify.com': 'spotify', 'soundcloud.com': 'soundcloud',
    'youtube.com/@': 'youtube', 'linktr.ee': 'linktree',
    'sextpanther.com': 'sextpanther', 'chaturbate.com': 'chaturbate',
    'loyalfans.com': 'loyalfans', 'fanvue.com': 'fanvue',
    'subscribestar.adult': 'subscribestar', 'clips4sale.com': 'clips4sale',
    'pornhub.com': 'pornhub', 'xvideos.com': 'xvideos', 'redgifs.com': 'redgifs',
    'literotica.com': 'literotica', 'archiveofourown.org': 'ao3',
    'etsy.com': 'etsy', 'paypal.me': 'paypal', 'cash.app': 'cashapp',
}


def fetch(url, timeout=20):
    try:
        r = subprocess.run(
            ['curl', '-sL', '--max-time', str(timeout), '-A', UA,
             '-H', 'Accept-Language: en-US,en;q=0.9', url],
            capture_output=True, text=True, timeout=timeout + 10)
        return r.stdout
    except Exception:
        return ''


def host_of(u):
    m = re.match(r'https?://([^/]+)', u or '')
    return m.group(1).lower().replace('www.', '') if m else ''


def is_junk(u):
    low = u.lower().rstrip('/')
    if len(u) < 12:
        return True
    for j in JUNK:
        if j.endswith('$'):
            if low.endswith(j[:-1]):
                return True
        elif j in low:
            return True
    return False


def extract(url):
    body = fetch(url)
    if not body:
        return []
    found = set(re.findall(
        r'https?://[A-Za-z0-9._%+\-]+/[A-Za-z0-9._%+\-/?#=&~]*', body))
    return sorted({u.rstrip('.,);"\'') for u in found if not is_junk(u)})


def resolve_tco(url):
    try:
        r = subprocess.run(
            ['curl', '-s', '-o', '/dev/null', '-w', '%{url_effective}',
             '--max-time', '15', '-A', UA, '-L', url],
            capture_output=True, text=True, timeout=20)
        return r.stdout.strip()
    except Exception:
        return ''


def main():
    rows = [json.loads(l) for l in open(os.path.join(HERE, 'following.jsonl'))]
    agg_urls, tcos = {}, {}
    for u in rows:
        su = u.get('url') or ''
        h = host_of(su)
        if h and any(h == a or h.endswith('.' + a) for a in AGG_HOSTS):
            agg_urls.setdefault(su, []).append(u['screen_name'])
        for t in re.findall(r'https?://t\.co/[A-Za-z0-9]+', u.get('description') or ''):
            tcos.setdefault(t, []).append(u['screen_name'])

    out = os.path.join(HERE, 'enrich.json')
    data = json.load(open(out)) if os.path.exists(out) else {'agg': {}, 'tco': {}}
    todo = [u for u in agg_urls if u not in data['agg']]
    print(f'aggregators: {len(agg_urls)} total, {len(todo)} to fetch', flush=True)
    with cf.ThreadPoolExecutor(max_workers=6) as ex:
        futs = {ex.submit(extract, u): u for u in todo}
        for i, f in enumerate(cf.as_completed(futs), 1):
            u = futs[f]
            try:
                data['agg'][u] = f.result()
            except Exception as e:
                data['agg'][u] = []
                print('ERR', u, e, flush=True)
            if i % 20 == 0:
                json.dump(data, open(out, 'w'), indent=0)
                print(i, '/', len(todo), flush=True)

    todo_t = [t for t in tcos if t not in data['tco']]
    print(f't.co: {len(tcos)} total, {len(todo_t)} to resolve', flush=True)
    with cf.ThreadPoolExecutor(max_workers=8) as ex:
        futs = {ex.submit(resolve_tco, t): t for t in todo_t}
        for i, f in enumerate(cf.as_completed(futs), 1):
            data['tco'][futs[f]] = f.result()
            if i % 40 == 0:
                json.dump(data, open(out, 'w'), indent=0)
    json.dump(data, open(out, 'w'), indent=0)
    print('done. agg', len(data['agg']), 'tco', len(data['tco']), flush=True)


if __name__ == '__main__':
    main()
