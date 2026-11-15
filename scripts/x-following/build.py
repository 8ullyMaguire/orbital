#!/usr/bin/env python3
"""Turn the X following harvest into Orbital rows (staging CSV + SQL).

Input : following.jsonl, enrich.json
Output: records.json (review), stage.csv (psql \\copy), import.sql
"""
import json, re, os, unicodedata, csv

HERE = os.path.dirname(os.path.abspath(__file__))
V = json.load(open(os.path.join(HERE, 'enrich.json')))

PALETTE = ["#7c5cff", "#3ec7a0", "#f2a65a", "#5aa9f2", "#f25aa9",
           "#c8f25a", "#f2735a", "#5af2e0", "#a95af2", "#f2d75a"]


def accent_for(handle):
    h = 0xcbf29ce484222325
    for b in handle.encode():
        h ^= b
        h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return PALETTE[h % len(PALETTE)]


ICONS = [("bsky.app", "bsky"), ("github.com", "github"), ("youtube.com", "youtube"),
         ("youtu.be", "youtube"), ("twitch.tv", "twitch"), ("x.com", "x"),
         ("twitter.com", "x"), ("discord", "discord"), ("reddit.com", "reddit"),
         ("ko-fi.com", "kofi"), ("patreon.com", "patreon"), ("bandcamp.com", "bandcamp"),
         ("spotify.com", "spotify"), ("steampowered.com", "steam"), ("itch.io", "itch"),
         ("substack.com", "pen"), ("medium.com", "pen"), ("goodreads.com", "book"),
         ("archiveofourown", "book"), ("instagram.com", "camera"),
         ("tiktok.com", "music"), ("soundcloud.com", "headphones")]


def icon_for(url):
    u = url.lower()
    for n, i in ICONS:
        if n in u:
            return i
    return "link"


def flat(s):
    return unicodedata.normalize('NFKC', s or '').lower()


TAGS = [
    ('bnwo', [r'\bbnwo\b', r'black new world order', r'\bbmwo\b', r'blacked']),
    ('interracial', [r'interracial', r'\bir\b couple', r'\bbmww?\b', r'\bbwm\b']),
    ('bbc', [r'\bbbc\b', r'big black cock']),
    ('qos', [r'\bqos\b', r'queen of spades', '♠', r'\bqosc?\b']),
    ('cuckold', [r'cuckold', r'\bcuck\b', r'cuckquean', r'\bstag\b', r'bull']),
    ('hotwife', [r'hotwife', r'hot wife', r'wife sharing', r'\bvixen\b']),
    ('snowbunny', [r'snow ?bunny']),
    ('chastity', [r'chastity', r'locktober', r'keyholder', r'\bcaged\b']),
    ('femdom', [r'femdom', r'female dom', r'\bdomme\b', r'domina', r'mistress',
                r'goddess', r'pro ?dom', r'female supremac']),
    ('findom', [r'findom', r'pay ?pig', r'human atm', r'tribute', r'financial domination']),
    ('sissy', [r'sissy', r'sissification']),
    ('feminization', [r'feminization', r'feminisation']),
    ('cbt', [r'\bcbt\b', r'ball ?bust']),
    ('pegging', [r'pegging', r'strap-?on']),
    ('sph', [r'\bsph\b', r'small (?:penis|cock)']),
    ('denial', [r'denial', r'\bedging\b']),
    ('goon', [r'goon']),
    ('captions', [r'caption', r'\bpms?\b', r'\bcaps\b']),
    ('hentai', [r'hentai', r'doujin', r'ahegao']),
    ('feet', [r'\bfeet\b', r'\bfoot\b', r'footjob', r'\btoes\b']),
    ('latex', [r'latex', r'\brubber\b', r'pvc fetish']),
    ('bdsm', [r'bdsm', r'bondage', r'submissive', r'dominant', r'dom/sub',
              r'\bkink', r'fetish', r'spank', r'sadist', r'sadomasoch']),
    ('joi', [r'\bjoi\b', r'jerk ?off instruction']),
    ('hypno', [r'hypno', r'bimbo']),
    ('cosplay', [r'cosplay']),
    ('anime', [r'anime', r'manga', r'waifu']),
    ('ai-art', [r'ai ?art', r'\bai nsfw\b', r'midjourney', r'stable diffusion', r'ai generated']),
    ('art', [r'illustrator', r'artist', r'\bdraw', r'\bart\b', r'sketch', r'commission']),
    ('photography', [r'photograph']),
    ('vtuber', [r'vtuber', r'vtubing']),
    ('streamer', [r'streamer', r'streaming', r'twitch']),
    ('youtuber', [r'youtuber', r'youtube']),
    ('musician', [r'musician', r'producer', r'\bdj\b', r'singer', r'\bband\b', r'music']),
    ('writer', [r'writer', r'author', r'poet', r'blogger', r'erotica', r'stories']),
    ('dev', [r'developer', r'programmer', r'software', r'\bcoder\b', r'\brust\b',
             r'\bpython\b', r'sysadmin', r'self-?host']),
    ('gamedev', [r'game ?dev', r'indie game']),
    ('milf', [r'\bmilf\b', r'mommy', r'mature']),
    ('trans', [r'\btrans', r'\bmtf\b', r'\bftm\b', r'tgirl', r'femboy']),
    ('amateur', [r'amateur']),
    ('exhibitionist', [r'exhibitionist', r'flashing', r'public play']),
    ('swinger', [r'swinger', r'swinging']),
    ('breeding', [r'breed', r'creampie']),
    ('gangbang', [r'gang ?bang', r'threesome', r'group sex']),
    ('pornstar', [r'porn ?star', r'\bavn\b', r'performer']),
    ('creator', [r'content creator', r'\bcreator\b', r'\bmodel\b', r'camgirl',
                 r'\bescort\b', r'onlyfans', r'\b0f\b']),
    ('onlyfans', [r'onlyfans', r'only fans', r'\b0f\b']),
    ('fansly', [r'fansly']),
    ('manyvids', [r'manyvids']),
    ('asmr', [r'asmr']),
    ('roleplay', [r'roleplay', r'\brp\b']),
    ('sexting', [r'sexting', r'\bsext\b']),
    ('dm-open', [r'dm ?(?:me|open|s open)', r'dms open']),
]

ADULT_DOMAINS = ('onlyfans.com', 'fansly.com', 'manyvids.com', 'chaturbate.com',
                 'loyalfans.com', 'sextpanther.com', 'fanvue.com', 'blacked.com',
                 'pornhub.com', 'xvideos.com', 'redgifs.com', 'throne.com',
                 'fansly.com', 'clips4sale.com', 'subscribestar.adult',
                 'youpay.bio', 'blackedraw', 'sxyprn', 'motherless', 'simpcity',
                 'justfor.fans', 'jff.')
ADULT_KW = ['nsfw', '18+', '🔞', 'porn', 'nude', 'bdsm', 'femdom', 'domme', 'domina',
            'cuck', 'hotwife', 'bnwo', 'bbc', 'qos', 'sissy', 'chastity', 'hentai',
            'goon', 'adult', 'escort', 'camgirl', 'latex', 'findom', 'tribute',
            'mistress', 'slave', 'cbt', 'pegging', 'gloryhole', 'gangbang', 'blowjob',
            'cum', 'cock', 'dick', 'tits', 'pussy', 'anal', 'fetish', 'kink',
            'swinger', 'milf', 'degrad', 'humiliat', 'hypno', 'denial', 'sph',
            'smut', 'lewd', 'erotic', 'sex', 'nudes', 'submissive', 'spank',
            'paypig', 'worship', 'snowbunny', 'blacked', 'interracial', 'breeding',
            'goddess', 'slut', 'whore', 'bimbo', 'daddy', 'mommy', 'mistress',
            'justfor', 'calcinelle', 'ballbust', '金蹴']
SFW_VERTICAL_KW = {
    'dev': ['developer', 'programmer', 'software engineer', 'rust', 'python',
            'open source', 'opensource', 'self-host', 'sysadmin', 'linux'],
    'art': ['illustrator', 'artist', 'concept art', 'painting', 'sculptor'],
    'music': ['musician', 'producer', 'composer', 'singer', 'bandcamp'],
    'writers': ['author', 'novelist', 'poet', 'blogger', 'journalist'],
    'youtube': ['youtuber', 'youtube channel', 'video essay'],
    'gaming': ['gamedev', 'game dev', 'speedrun', 'gaming'],
    'vtubers': ['vtuber', 'vtubing'],
    'podcasters': ['podcast', 'podcaster'],
    'indie-web': ['indie web', 'indieweb', 'personal site'],
}


def shape_handle(raw):
    h = raw.strip().lstrip('@').lower()
    h = re.sub(r'[^a-z0-9.-]', '-', h)
    h = re.sub(r'-{2,}', '-', h).strip('.-')
    return h[:30]


def clean_bio(text):
    t = re.sub(r'https?://t\.co/\S+', '', text or '')
    t = re.sub(r'[ \t]+', ' ', t)
    t = re.sub(r'\n{3,}', '\n\n', t).strip()
    return t[:600]


def classify(text, urls):
    """(vertical, [tags]) — adult by default; SFW only on a clean, strong signal."""
    tl = flat(text)
    tags = []
    for tag, pats in TAGS:
        if any(re.search(p, tl) for p in pats):
            tags.append(tag)
    low_urls = ' '.join(urls).lower()
    ADULT_KINDS = {'onlyfans', 'fansly', 'manyvids', 'chaturbate', 'loyalfans',
                   'sextpanther', 'fanvue', 'subscribestar', 'clips4sale',
                   'pornhub', 'redgifs', 'justforfans'}
    ADULT_TAGS = {'bnwo', 'bbc', 'qos', 'cuckold', 'hotwife', 'snowbunny',
                  'chastity', 'femdom', 'findom', 'sissy', 'feminization', 'cbt',
                  'pegging', 'sph', 'denial', 'goon', 'captions', 'hentai', 'joi',
                  'hypno', 'breeding', 'gangbang', 'exhibitionist', 'swinger',
                  'onlyfans', 'fansly', 'manyvids', 'pornstar', 'milf'}
    adult = (any(k in tl for k in ADULT_KW)
             or any(d in low_urls for d in ADULT_DOMAINS)
             or any((social_kind(u) in ADULT_KINDS) for u in urls)
             or any(t in ADULT_TAGS for t in tags))
    vertical = 'nsfw'
    if not adult:
        for slug, kws in SFW_VERTICAL_KW.items():
            if any(k in tl for k in kws):
                vertical = slug
                break
    if adult:
        tags = [t for t in tags if t not in SFW_VERTICAL_KW] or tags
        if 'creator' not in tags and any(d in low_urls for d in ADULT_DOMAINS):
            tags.append('creator')
    else:
        tags = [t for t in tags if t not in ('bnwo', 'bbc', 'qos', 'cuckold', 'hotwife',
                                             'snowbunny', 'chastity', 'femdom', 'findom',
                                             'sissy', 'cbt', 'sph', 'goon', 'captions',
                                             'hentai', 'joi', 'hypno', 'breeding',
                                             'gangbang', 'exhibitionist', 'swinger',
                                             'onlyfans', 'fansly', 'manyvids', 'milf')]
    return vertical, tags[:8]


SOCIAL_LABEL = {
    'instagram': 'Instagram', 'tiktok': 'TikTok', 'bsky': 'Bluesky',
    'youtube': 'YouTube', 'twitch': 'Twitch', 'fansly': 'Fansly',
    'onlyfans': 'OnlyFans', 'patreon': 'Patreon', 'discord': 'Discord',
    'reddit': 'Reddit', 'tumblr': 'Tumblr', 'pixiv': 'Pixiv', 'kofi': 'Ko-fi',
    'throne': 'Throne', 'manyvids': 'ManyVids', 'fetlife': 'FetLife',
    'snapchat': 'Snapchat', 'telegram': 'Telegram', 'spotify': 'Spotify',
    'soundcloud': 'SoundCloud', 'sextpanther': 'SextPanther',
    'chaturbate': 'Chaturbate', 'loyalfans': 'LoyalFans', 'fanvue': 'Fanvue',
    'subscribestar': 'SubscribeStar', 'clips4sale': 'Clips4Sale',
    'pornhub': 'Pornhub', 'redgifs': 'RedGifs', 'literotica': 'Literotica',
    'ao3': 'AO3', 'etsy': 'Etsy', 'justforfans': 'JustFor.Fans', 'paypal': 'PayPal', 'cashapp': 'Cash App',
}
TARGET = ('instagram.com', 'tiktok.com', 'bsky.app', 'youtube.com/@', 'youtu.be',
          'twitch.tv', 'fansly.com', 'onlyfans.com', 'patreon.com', 'discord.gg',
          'reddit.com/user', 'reddit.com/r/', 'tumblr.com', 'pixiv.net',
          'ko-fi.com', 'throne.com', 'manyvids.com', 'fetlife.com',
          'snapchat.com/add', 't.me', 'sextpanther.com', 'chaturbate.com',
          'loyalfans.com', 'fanvue.com', 'subscribestar.adult', 'clips4sale.com',
          'soundcloud.com', 'spotify.com', 'literotica.com', 'archiveofourown.org',
          'etsy.com', 'justfor.fans')


def social_kind(url):
    u = url.lower()
    for dom, kind in (('instagram.com', 'instagram'), ('tiktok.com', 'tiktok'),
                      ('bsky.app', 'bsky'), ('youtube.com', 'youtube'),
                      ('youtu.be', 'youtube'), ('twitch.tv', 'twitch'),
                      ('fansly.com', 'fansly'), ('onlyfans.com', 'onlyfans'),
                      ('patreon.com', 'patreon'), ('discord.gg', 'discord'),
                      ('reddit.com', 'reddit'), ('tumblr.com', 'tumblr'),
                      ('pixiv.net', 'pixiv'), ('ko-fi.com', 'kofi'),
                      ('throne.com', 'throne'), ('manyvids.com', 'manyvids'),
                      ('fetlife.com', 'fetlife'), ('snapchat.com', 'snapchat'),
                      ('t.me', 'telegram'), ('sextpanther.com', 'sextpanther'),
                      ('chaturbate.com', 'chaturbate'), ('loyalfans.com', 'loyalfans'),
                      ('fanvue.com', 'fanvue'), ('subscribestar', 'subscribestar'),
                      ('clips4sale.com', 'clips4sale'), ('soundcloud.com', 'soundcloud'),
                      ('spotify.com', 'spotify'), ('literotica.com', 'literotica'),
                      ('archiveofourown.org', 'ao3'), ('etsy.com', 'etsy'),
                      ('justfor.fans', 'justforfans'),
                      ('paypal.me', 'paypal'), ('cash.app', 'cashapp')):
        if dom in u:
            return kind
    return None


def norm_url(u):
    u = u.strip().rstrip('.,);"\'')
    if u.startswith('//'):
        u = 'https:' + u
    if not re.match(r'https?://', u):
        u = 'https://' + u
    return u


def build():
    rows = [json.loads(l) for l in open(os.path.join(HERE, 'following.jsonl'))]
    recs, skipped = [], []
    for u in rows:
        if u['protected']:
            skipped.append((u['screen_name'], 'protected'))
            continue
        handle = shape_handle(u['screen_name'])
        if len(handle) < 2:
            skipped.append((u['screen_name'], 'handle too short after normalising'))
            continue
        bio = clean_bio(u['description'])
        name = re.sub(r'\s+', ' ', u['name'] or handle).strip()[:80] or handle
        if not re.search(r'[0-9a-z]', name.lower()):
            name = u['screen_name']
        x_url = f"https://x.com/{u['screen_name']}"
        bio_url = norm_url(u['url']) if u.get('url') else ''
        harvested = V['agg'].get(u.get('url') or '', []) if bio_url else []
        tco = [V['tco'].get(t, '') for t in re.findall(
            r'https?://t\.co/[A-Za-z0-9]+', u.get('description') or '')]

        seen, links = set(), []

        def add(url, label):
            url = norm_url(url)
            m = re.match(r'https?://([^/]+)(/.*)?$', url)
            if m and (m.group(2) or '/') in ('', '/') and 'x.com' not in m.group(1):
                return
            key = re.sub(r'^https?://(www\.)?', '', url.lower()).rstrip('/')
            if not key or key in seen or len(links) >= 10:
                return
            seen.add(key)
            links.append({'url': url, 'label': label[:60], 'icon': icon_for(url)})

        add(x_url, 'X (Twitter)')
        if bio_url and 't.co' not in bio_url:
            kind = social_kind(bio_url)
            agg = any(a in bio_url.lower() for a in (
                'linktr.ee', 'allmylinks', 'beacons.', 'link.me', 'hoo.be', 'vxn.link',
                'youpay.bio', 'sent.bio', 'oopsie.bio', 'bio.site', 'linkr.bio',
                'carrd.co', 'milkshake', 'komi.io', 'lnk.bio', 'bit.ly'))
            add(bio_url, 'All my links' if agg else (SOCIAL_LABEL.get(kind or '', u['name'][:40]))
                if kind else 'Links')
        for c in tco:
            if c and 't.co/' not in c and 'x.com' not in c:
                k = social_kind(c)
                add(c, SOCIAL_LABEL.get(k or '', 'Link'))
        for s in harvested:
            if not any(t in s.lower() for t in TARGET):
                continue
            k = social_kind(s)
            add(s, SOCIAL_LABEL.get(k or '', 'Link'))

        text = ' '.join([name, bio] + [l['url'] for l in links])
        vertical, tags = classify(text, [l['url'] for l in links])
        if vertical != 'nsfw' and not tags:
            tags = [{'gaming': 'gaming', 'music': 'music', 'art': 'art',
                     'dev': 'dev', 'writers': 'writing', 'youtube': 'youtube',
                     'vtubers': 'vtuber', 'podcasters': 'podcast',
                     'indie-web': 'indie-web'}.get(vertical, vertical)]
        if vertical == 'nsfw' and not tags:
            tags = ['creator'] if any('onlyfans' in l['url'] or 'fansly' in l['url']
                                      for l in links) else ['adult']
        recs.append({
            'handle': handle, 'x_handle': u['screen_name'], 'display_name': name,
            'bio': bio, 'accent': accent_for(handle),
            'avatar_url': None if u.get('default_profile_image') else
            u['profile_image_url_https'].replace('_normal', '_200x200'),
            'vertical': vertical, 'tags': tags, 'links': links,
            'followers': u['followers_count'],
        })
    json.dump(recs, open(os.path.join(HERE, 'records.json'), 'w'), indent=1)
    print('records', len(recs), 'skipped', len(skipped), skipped[:5])
    import collections
    print('verticals', collections.Counter(r['vertical'] for r in recs))
    print('links/profile', collections.Counter(len(r['links']) for r in recs).most_common(8))
    print('no tags', sum(1 for r in recs if not r['tags']))
    print('tagged', collections.Counter(t for r in recs for t in r['tags']).most_common(25))

    # staging CSV for psql \copy
    with open(os.path.join(HERE, 'stage.csv'), 'w', newline='') as f:
        w = csv.writer(f, lineterminator='\n')
        for r in recs:
            w.writerow([r['handle'], r['display_name'], r['bio'], r['accent'],
                        r['avatar_url'] or '', r['vertical'],
                        json.dumps(r['tags']), json.dumps(r['links'])])
    return recs


if __name__ == '__main__':
    build()
