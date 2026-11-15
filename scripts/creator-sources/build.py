#!/usr/bin/env python3
"""Turn stash-box performer dumps into Orbital rows.

    ./build.py --stage stage-creators.csv

Inputs : stashdb_all.jsonl, fansdb_all.jsonl (sweep.py), orbital-ident.txt
         (handle<TAB>url for every existing member+link, from psql)
Outputs: creators.json (what was decided, for review), stage-creators.csv (psql \\copy)

The rule is "a creator is a performer with somewhere of their own to go": IAFD,
Boobpedia, IMDb and studio pages are evidence that a person exists, not a profile
to send a reader to, so a performer whose only URLs are those is not imported.
"""
import argparse
import csv
import json
import os
import re
import unicodedata
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))

# host -> (kind, label, icon)
CREATOR_HOSTS = {
    'x.com': ('x', 'X (Twitter)', 'x'),
    'twitter.com': ('x', 'X (Twitter)', 'x'),
    'bsky.app': ('bsky', 'Bluesky', 'bsky'),
    'instagram.com': ('instagram', 'Instagram', 'camera'),
    'tiktok.com': ('tiktok', 'TikTok', 'music'),
    'onlyfans.com': ('onlyfans', 'OnlyFans', 'link'),
    'fansly.com': ('fansly', 'Fansly', 'link'),
    'manyvids.com': ('manyvids', 'ManyVids', 'link'),
    'justfor.fans': ('justforfans', 'JustFor.Fans', 'link'),
    'loyalfans.com': ('loyalfans', 'LoyalFans', 'link'),
    'fanvue.com': ('fanvue', 'Fanvue', 'link'),
    'chaturbate.com': ('chaturbate', 'Chaturbate', 'link'),
    'sextpanther.com': ('sextpanther', 'SextPanther', 'link'),
    'subscribestar.adult': ('subscribestar', 'SubscribeStar', 'link'),
    'clips4sale.com': ('clips4sale', 'Clips4Sale', 'link'),
    'faphouse.com': ('faphouse', 'FapHouse', 'link'),
    'patreon.com': ('patreon', 'Patreon', 'patreon'),
    'ko-fi.com': ('kofi', 'Ko-fi', 'kofi'),
    'throne.com': ('throne', 'Throne', 'link'),
    'reddit.com': ('reddit', 'Reddit', 'reddit'),
    'tumblr.com': ('tumblr', 'Tumblr', 'link'),
    'pixiv.net': ('pixiv', 'Pixiv', 'link'),
    'youtube.com': ('youtube', 'YouTube', 'youtube'),
    'youtu.be': ('youtube', 'YouTube', 'youtube'),
    'twitch.tv': ('twitch', 'Twitch', 'twitch'),
    'snapchat.com': ('snapchat', 'Snapchat', 'link'),
    't.me': ('telegram', 'Telegram', 'link'),
    'telegram.me': ('telegram', 'Telegram', 'link'),
    'soundcloud.com': ('soundcloud', 'SoundCloud', 'headphones'),
    'spotify.com': ('spotify', 'Spotify', 'spotify'),
    'fetlife.com': ('fetlife', 'FetLife', 'link'),
    'linktr.ee': ('linkinbio', 'All my links', 'link'),
    'allmylinks.com': ('linkinbio', 'All my links', 'link'),
    'beacons.ai': ('linkinbio', 'All my links', 'link'),
    'beacons.page': ('linkinbio', 'All my links', 'link'),
    'link.me': ('linkinbio', 'All my links', 'link'),
    'hoo.be': ('linkinbio', 'All my links', 'link'),
    'vxn.link': ('linkinbio', 'All my links', 'link'),
    'lnk.bio': ('linkinbio', 'All my links', 'link'),
    'solo.to': ('linkinbio', 'All my links', 'link'),
    'bio.link': ('linkinbio', 'All my links', 'link'),
    'linkr.bio': ('linkinbio', 'All my links', 'link'),
    'milkshake.app': ('linkinbio', 'All my links', 'link'),
    'komi.io': ('linkinbio', 'All my links', 'link'),
    'taplink.cc': ('linkinbio', 'All my links', 'link'),
    'carrd.co': ('linkinbio', 'All my links', 'link'),
    'bio.site': ('linkinbio', 'All my links', 'link'),
    'youpay.bio': ('linkinbio', 'All my links', 'link'),
    'fansone.co': ('fansone', 'FansOne', 'link'),
    'passes.com': ('passes', 'Passes', 'link'),
    'my.club': ('linkinbio', 'All my links', 'link'),
    'facebook.com': ('facebook', 'Facebook', 'link'),
    'fb.com': ('facebook', 'Facebook', 'link'),
    'threads.net': ('threads', 'Threads', 'link'),
    'kink.com': ('kink', 'Kink', 'link'),
    'etsy.com': ('etsy', 'Etsy', 'link'),
    'amazon.com': ('wishlist', 'Wishlist', 'link'),
    'wishlist': ('wishlist', 'Wishlist', 'link'),
    'discord.gg': ('discord', 'Discord', 'discord'),
}

# A performer whose only URLs are these does not get a profile: they are records,
# not places the performer publishes.
EVIDENCE_HOSTS = (
    'iafd.com', 'boobpedia.com', 'babepedia.com', 'indexxx.com', 'egafd.com',
    'thenude.com', 'freeones.com', 'gevi.org', 'imdb.com', 'theporndb.net',
    'tpdb.io', 'stashdb.org', 'wikipedia.org', 'avn.com', 'xbiz.com',
    'adultfilmdatabase.com', 'eafd.com', 'adultdvdtalk.com', 'xfanz.com',
    'data18.com', 'iafd', 'afdb', 'pornstarsdatabase', 'modelhub.com',
    'pornhub.com', 'xvideos.com', 'xhamster.com', 'spankbang.com', 'eporner.com',
    'hqporner.com', 'redtube.com', 'youporn.com', 'beeg.com', 'txxx.com',
    'hclips.com', 'tnaflix.com', 'motherless.com', 'nhentai.net', 'mydirtyhobby',
)

AGG_SUFFIX = ('/api', '/v1')
# A route word is not an identity: `manyvids.com/profile` names a page type, and matching it made
# 15,132 performers look like they were already on Orbital.
ROUTE_WORDS = {'profile', 'profiles', 'user', 'users', 'u', 'p', 'add', 'activity', 'studio',
               'studios', 'video', 'videos', 'login', 'signup', 'i', 'home', 'watch', 'channel',
               'c', 'model', 'models', 'shop', 'store', 'tour', 'join', 'about', 'search',
               'view', 'photo', 'photos', 'gallery', 'post', 'posts', 'reel', 'shorts'}


def flat(s):
    return unicodedata.normalize('NFKC', s or '').lower()


def slugify(name, fallback='creator'):
    s = unicodedata.normalize('NFKD', name or '').encode('ascii', 'ignore').decode()
    s = re.sub(r"[^a-z0-9]+", '-', s.lower()).strip('-')
    return (s or fallback)[:30]


PALETTE = ["#7c5cff", "#3ec7a0", "#f2a65a", "#5aa9f2", "#f25aa9",
           "#c8f25a", "#f2735a", "#5af2e0", "#a95af2", "#f2d75a"]


def accent_for(handle):
    h = 0xcbf29ce484222325
    for b in handle.encode():
        h ^= b
        h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return PALETTE[h % len(PALETTE)]


def host_of(url):
    m = re.match(r'https?://([^/]+)', (url or '').strip())
    if not m:
        return '', ''
    h = m.group(1).lower().replace('www.', '')
    rest = url.split(m.group(1), 1)[1] if m.group(1) in url else ''
    return h, rest.strip('/')


def classify(url):
    """(kind, label, icon, identity-key) or None when the URL is not a creator surface."""
    h, rest = host_of(url)
    if not h:
        return None
    for ev in EVIDENCE_HOSTS:
        if ev in h:
            return None
    for dom, (kind, label, icon) in CREATOR_HOSTS.items():
        if h == dom or h.endswith('.' + dom):
            if kind == 'x':
                handle = rest.split('/')[0] if rest else ''
                if not handle or handle.lower() in ('i', 'home', 'intent', 'share'):
                    return None
                return (kind, label, icon, 'x:' + handle.lower().lstrip('@'))
            if kind == 'linkinbio':
                first = rest.split('/')[0].split('?')[0].lower()
                return (kind, label, icon, f'{dom}:{first}' if first else '')
            first = rest.split('/')[0].split('?')[0].lower()
            if not first or first in ROUTE_WORDS:
                return (kind, label, icon, '')
            return (kind, label, icon, f'{dom}:{first}')
    # An official/personal site: keep it, host is the identity.
    return ('website', 'Website', 'link', f'site:{h}')


def facts(p):
    """A short, factual bio. Nothing here is invented: every clause is a column."""
    role = 'Performer'
    if (p.get('gender') or '').startswith('TRANSGENDER'):
        role = 'Trans performer'
    bits = [role + '.']
    n = p.get('scene_count') or 0
    start = p.get('career_start_year')
    if n and start:
        bits.append(f'{n:,} scene{"s" if n != 1 else ""} catalogued, working since {start}.')
    elif n:
        bits.append(f'{n:,} scene{"s" if n != 1 else ""} catalogued.')
    elif start:
        bits.append(f'Working since {start}.')
    country = (p.get('country') or '').strip()
    if country:
        bits.append(f'Based in {country}.')
    return ' '.join(bits).strip()


def load_existing(path):
    handles, keys = set(), set()
    for line in open(path):
        parts = line.rstrip('\n').split('|')
        if not parts or not parts[0]:
            continue
        handles.add(parts[0])
        if len(parts) > 1 and parts[1]:
            c = classify(parts[1])
            if c:
                keys.add(c[3])
    return handles, keys


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--stage', default='stage-creators.csv')
    ap.add_argument('--records', default='creators.json')
    ap.add_argument('--ident', default=os.path.join(HERE, 'orbital-ident.txt'))
    ap.add_argument('--limit', type=int, default=0, help='stop after N new creators (dry runs)')
    ap.add_argument('--inputs', default='',
                    help='site:file pairs, comma separated (default: stashdb_all.jsonl,fansdb_all.jsonl)')
    args = ap.parse_args()

    handles, keys = load_existing(args.ident)
    print(f'existing: {len(handles)} handles, {len(keys)} link identities')

    pairs = [('stashdb', 'stashdb_all.jsonl'), ('fansdb', 'fansdb_all.jsonl')]
    if args.inputs:
        pairs = [tuple(x.split(':', 1)) for x in args.inputs.split(',') if ':' in x]
    performers, per_source = {}, Counter()
    for source, fname in pairs:
        path = fname if os.path.isabs(fname) else os.path.join(HERE, fname)
        if not os.path.exists(path):
            continue
        for line in open(path):
            try:
                p = json.loads(line)
            except Exception:
                continue
            pid = p.get('id')
            if not pid:
                continue
            p['_source'] = source
            per_source[source] += 1
            performers.setdefault(pid, p)
    print('performers read:', dict(per_source), '->', len(performers), 'distinct ids')

    stats = Counter()
    recs, used_handles = [], set(handles)
    for p in performers.values():
        kept, seen = [], set()
        for u in p.get('urls') or []:
            c = classify(u.get('url') or '')
            if not c:
                continue
            kind, label, icon, key = c
            if key in seen or (key == '' and kind == 'website' and len(seen) > 8):
                continue
            seen.add(key if key else f'_anon{len(seen)}')
            kept.append({'url': (u.get('url') or '').strip(), 'label': label,
                         'icon': icon, 'key': key, 'kind': kind})
        recognised = [k for k in kept if not k['key'].startswith('site:')]
        if not recognised:
            stats['no creator social'] += 1
            continue
        if any(k['key'] and k['key'] in keys for k in kept):
            stats['already on orbital'] += 1
            continue
        # Handle: the X handle if they have one, else the first creator handle, else the name.
        handle = ''
        for k in kept:
            if k['key'].startswith('x:'):
                handle = slugify(k['key'][2:])
                break
        if not handle:
            for k in kept:
                if ':' in k['key'] and not k['key'].startswith('site:'):
                    handle = slugify(k['key'].split(':', 1)[1])
                    if handle:
                        break
        if not handle or len(handle) < 2:
            handle = slugify(p.get('name') or '')
        if len(handle) < 2:
            # p.get('name') can be a single character; the site's grammar wants two.
            handle = f"{handle or 'x'}-{p['_source']}"[:30]
        base = handle
        n = 1
        while handle in used_handles:
            n += 1
            handle = f'{base}-{n}'[:30]
        used_handles.add(handle)

        links = [{'url': k['url'], 'label': k['label'], 'icon': k['icon']} for k in kept][:9]
        if p.get('_source') == 'stashdb':
            links.append({'url': f"https://stashdb.org/performers/{p['id']}",
                          'label': 'StashDB', 'icon': 'book'})
            keys.add(f"stashdb:{p['id']}")
        else:
            links.append({'url': f"https://fansdb.cc/performers/{p['id']}",
                          'label': 'FansDB', 'icon': 'book'})
            keys.add(f"fansdb:{p['id']}")

        tags = ['performer']
        kinds = {k['kind'] for k in kept}
        for kind in ('onlyfans', 'fansly', 'manyvids', 'justforfans', 'loyalfans',
                     'chaturbate', 'fanvue', 'clips4sale', 'reddit', 'patreon',
                     'pixiv', 'tiktok', 'instagram', 'youtube', 'twitch', 'fetlife'):
            if kind in kinds and kind not in tags:
                tags.append(kind)
        if (p.get('gender') or '').startswith('TRANSGENDER'):
            tags.append('trans')
        if (p.get('scene_count') or 0) < 10 and ('onlyfans' in kinds or 'fansly' in kinds):
            tags.append('amateur')
        if (p.get('scene_count') or 0) >= 100:
            tags.append('pornstar')

        name = (p.get('name') or handle).strip()[:80]
        if p.get('disambiguation'):
            name = f'{name} ({p["disambiguation"]})'[:80]
        recs.append({
            'handle': handle, 'display_name': name, 'bio': facts(p),
            'vertical': 'nsfw', 'tags': tags[:8], 'links': links,
            'avatar_url': (p.get('images') or [{}])[0].get('url') if p.get('images') else None,
            'source': p['_source'], 'source_id': p['id'], 'name': p.get('name'),
            'gender': p.get('gender'), 'scene_count': p.get('scene_count'),
        })
        if args.limit and len(recs) >= args.limit:
            break

    print('skipped:', dict(stats))
    print('new creators:', len(recs), Counter(r['source'] for r in recs))
    print('links per profile:', Counter(len(r['links']) for r in recs).most_common(6))
    print('tags:', Counter(t for r in recs for t in r['tags']).most_common(15))
    json.dump(recs, open(os.path.join(HERE, args.records), 'w'), indent=1)

    with open(os.path.join(HERE, args.stage), 'w', newline='') as f:
        w = csv.writer(f, lineterminator='\n')
        for r in recs:
            w.writerow([r['handle'], r['display_name'], r['bio'], accent_for(r['handle']),
                        r['avatar_url'] or '', r['vertical'],
                        json.dumps(r['tags']), json.dumps(r['links'])])
    print('staged ->', args.stage)


if __name__ == '__main__':
    main()
