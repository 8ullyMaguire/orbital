//! Small HTML fragments shared by the templates: icon glyphs, avatars, the
//! error page. Everything user-supplied that goes through here is escaped.

use crate::text::{esc, initials};

pub const SITE_NAME: &str = "Orbital";
pub const SITE_TAGLINE: &str = "A directory of people, sorted by what they make";

/// One stroke-icon set for the whole site. `viewBox` 24, currentColor, 1.7px —
/// a consistent weight reads as a design rather than a pile of brand logos,
/// and it costs no network request.
pub fn icon_svg(name: &str) -> String {
    let inner = match name {
        "game" => {
            r#"<path d="M7 12h4M9 10v4"/><circle cx="16" cy="11" r="1"/><circle cx="18" cy="13.5" r="1"/><path d="M7.5 7h9a4.5 4.5 0 0 1 4.4 5.4l-.9 4A3 3 0 0 1 17 18.5l-1.7-1.8H8.7L7 18.5a3 3 0 0 1-3-2.1l-.9-4A4.5 4.5 0 0 1 7.5 7Z"/>"#
        }
        "mic" => {
            r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3M9 21h6"/>"#
        }
        "video" => r#"<rect x="3" y="6" width="13" height="12" rx="3"/><path d="m16 11 5-3v8l-5-3"/>"#,
        "music" => {
            r#"<path d="M9 18V6l10-2v11"/><circle cx="6.5" cy="18" r="2.5"/><circle cx="16.5" cy="15" r="2.5"/>"#
        }
        "brush" => {
            r#"<path d="M14 4l6 6-8.5 8.5a3 3 0 0 1-2 .9H5l1-4.5a3 3 0 0 1 .9-2Z"/><path d="m12 6 6 6"/>"#
        }
        "code" => r#"<path d="m9 8-4 4 4 4M15 8l4 4-4 4"/>"#,
        "pen" => r#"<path d="M4 20l4-1 10-10a2.1 2.1 0 0 0-3-3L5 16Z"/><path d="m14 5 3 3"/>"#,
        "mask" => {
            r#"<path d="M3 11a9 9 0 0 1 18 0v2a4 4 0 0 1-4 4h-2l-3-2-3 2H7a4 4 0 0 1-4-4Z"/><circle cx="9" cy="11.5" r="1.2"/><circle cx="15" cy="11.5" r="1.2"/>"#
        }
        "globe" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.4 3.8 5.5 3.8 9S14.5 18.6 12 21c-2.5-2.4-3.8-5.5-3.8-9S9.5 5.4 12 3Z"/>"#
        }
        "flame" => {
            r#"<path d="M12 21c3.9 0 6-2.4 6-5.6 0-4.3-4.2-6.1-4.2-9.4 0-1-.3-2-.8-3-2.3 2.2-3.4 4.4-3.4 6.6 0 1.6.5 2.5.5 3.4 0 1-.8 1.7-1.7 1.7-1 0-1.6-.9-1.6-2.2C5.4 13.9 4 15.4 4 17c0 2.3 3 4 8 4Z"/>"#
        }
        "dot" => r#"<circle cx="12" cy="12" r="4"/>"#,
        "link" => {
            r#"<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>"#
        }
        "bsky" => {
            r#"<path d="M7 5.5c2.4 1.6 4 3.6 5 5.6 1-2 2.6-4 5-5.6 1.6-1 3-.4 3 1.6 0 2.6-1.2 6.4-2.6 8-1 1.1-2.4 1.3-3.6.4l-1.8-1.4-1.8 1.4c-1.2.9-2.6.7-3.6-.4C5.2 13.5 4 9.7 4 7.1c0-2 1.4-2.6 3-1.6Z"/>"#
        }
        "rss" => r#"<path d="M5 19h.01"/><path d="M4 11a9 9 0 0 1 9 9M4 4a16 16 0 0 1 16 16"/>"#,
        "github" => {
            r#"<path d="M9 19c-4 1.2-4-2.2-6-2.8m12 5v-3.6c0-1 .1-1.4-.5-2 2.3-.3 4.5-1.2 4.5-5.2a4 4 0 0 0-1.1-2.8 3.7 3.7 0 0 0-.1-2.8s-.9-.3-3 1.1a10.4 10.4 0 0 0-5.6 0C7.1 4.5 6.2 4.8 6.2 4.8a3.7 3.7 0 0 0-.1 2.8A4 4 0 0 0 5 10.4c0 4 2.2 4.9 4.5 5.2-.6.6-.6 1.2-.6 2V21"/>"#
        }
        "youtube" => {
            r#"<rect x="2.5" y="6" width="19" height="12" rx="4"/><path d="m10.5 9.5 4.5 2.5-4.5 2.5Z"/>"#
        }
        "twitch" => {
            r#"<path d="M4 4h16v10l-4 4h-3l-2.5 2.5H9V18H4Z"/><path d="M11 8v4M15 8v4"/>"#
        }
        "x" => r#"<path d="M4 4l16 16M20 4L4 20"/>"#,
        "discord" => {
            r#"<path d="M8 8a13 13 0 0 0-4 12l3 1 2-3h6l2 3 3-1a13 13 0 0 0-4-12Z"/><circle cx="9.5" cy="14" r="1"/><circle cx="14.5" cy="14" r="1"/>"#
        }
        "reddit" => {
            r#"<circle cx="12" cy="13" r="7"/><circle cx="9.5" cy="13" r=".8"/><circle cx="14.5" cy="13" r=".8"/><path d="M9 16c1.6 1 4.4 1 6 0M12 6V3m0 0 4 1"/>"#
        }
        "kofi" => {
            r#"<path d="M4 8h12v6a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4Z"/><path d="M16 10h2.5a2.5 2.5 0 0 1 0 5H16M7 3v2M11 3v2"/>"#
        }
        "patreon" => r#"<circle cx="14.5" cy="9.5" r="5.5"/><path d="M4 4v16"/>"#,
        "bandcamp" => {
            r#"<path d="M3 17 9 7h12l-6 10Z"/>"#
        }
        "spotify" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M7.5 9.5c3.5-1 7-.6 9.5 1M8 12.5c3-.8 5.8-.4 8 1M8.5 15.5c2.4-.6 4.6-.3 6.5.8"/>"#
        }
        "steam" => {
            r#"<circle cx="12" cy="12" r="9"/><circle cx="15" cy="9.5" r="2.5"/><path d="m3 15 5 2"/>"#
        }
        "itch" => {
            r#"<rect x="3" y="8" width="18" height="9" rx="3"/><path d="M8 12.5h2M18 11h.01"/><path d="M7 17v3l3-2 2 1 2-1 3 2v-3"/>"#
        }
        "book" => {
            r#"<path d="M4 5a2 2 0 0 1 2-2h5v18H6a2 2 0 0 1-2-2Z"/><path d="M20 5a2 2 0 0 0-2-2h-5v18h5a2 2 0 0 0 2-2Z"/>"#
        }
        "camera" => {
            r#"<path d="M4 8h3l2-2h6l2 2h3v11H4Z"/><circle cx="12" cy="13" r="3"/>"#
        }
        "heart" => {
            r#"<path d="M12 20s-7-4.4-7-9a4 4 0 0 1 7-2.6A4 4 0 0 1 19 11c0 4.6-7 9-7 9Z"/>"#
        }
        "star" => {
            r#"<path d="m12 4 2.5 5 5.5.8-4 3.9 1 5.5-5-2.7-5 2.7 1-5.5-4-3.9 5.5-.8Z"/>"#
        }
        "mail" => r#"<rect x="3" y="5" width="18" height="14" rx="3"/><path d="m4 7 8 6 8-6"/>"#,
        "shop" => {
            r#"<path d="M5 8h14l-1 12H6Z"/><path d="M9 8V6a3 3 0 0 1 6 0v2"/>"#
        }
        "forum" => {
            r#"<path d="M4 5h11v8H9l-4 3Z"/><path d="M15 9h5v7h-3l-2 2Z"/>"#
        }
        "headphones" => {
            r#"<path d="M4 14v-2a8 8 0 0 1 16 0v2"/><rect x="3" y="13" width="4" height="7" rx="2"/><rect x="17" y="13" width="4" height="7" rx="2"/>"#
        }
        "calendar" => {
            r#"<rect x="3" y="5" width="18" height="16" rx="3"/><path d="M8 3v4M16 3v4M3 11h18"/>"#
        }
        "coffee" => {
            r#"<path d="M4 8h12v6a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4Z"/><path d="M16 10h2a2 2 0 0 1 0 4h-2M7 4v2M11 4v2"/>"#
        }
        "thread" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M9 16c-1.5 0-2.5-1.2-2.5-3.2C6.5 10 8.5 8 11.6 8c3 0 4.9 1.7 4.9 4.4 0 2-1 3.2-2.3 3.2-1 0-1.6-.6-1.6-1.5 0-1.2 1.4-1.4 1.4-2.6 0-.8-.7-1.3-1.8-1.3-1.7 0-2.9 1.4-2.9 3.3 0 1.5.9 2.5 2.3 2.5"/>"#
        }
        "doc" => r#"<path d="M6 3h8l4 4v14H6Z"/><path d="M14 3v4h4M9 12h6M9 16h6"/>"#,
        "palette" => {
            r#"<path d="M12 3a9 9 0 1 0 0 18c1.6 0 2-1 1.4-2-.7-1.3.2-2.5 1.6-2.5H18a3 3 0 0 0 3-3c0-5.8-4.5-10.5-9-10.5Z"/><circle cx="8" cy="10" r="1"/><circle cx="12" cy="7.5" r="1"/><circle cx="16" cy="10" r="1"/>"#
        }
        "paw" => {
            r#"<circle cx="7" cy="9" r="1.8"/><circle cx="12" cy="7" r="1.8"/><circle cx="17" cy="9" r="1.8"/><path d="M12 20c-3 0-5-1.7-5-3.8 0-1.6 1.4-2.7 3-3.4a4.6 4.6 0 0 1 4 0c1.6.7 3 1.8 3 3.4 0 2.1-2 3.8-5 3.8Z"/>"#
        }
        _ => r#"<circle cx="12" cy="12" r="8"/><path d="M12 4v16"/>"#,
    };
    format!(
        r#"<svg class="ico" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">{inner}</svg>"#
    )
}

pub fn is_known_icon(name: &str) -> bool {
    matches!(
        name,
        "game"
            | "mic"
            | "video"
            | "music"
            | "brush"
            | "code"
            | "pen"
            | "mask"
            | "globe"
            | "flame"
            | "dot"
            | "link"
            | "bsky"
            | "rss"
            | "github"
            | "youtube"
            | "twitch"
            | "x"
            | "discord"
            | "reddit"
            | "kofi"
            | "patreon"
            | "bandcamp"
            | "spotify"
            | "steam"
            | "itch"
            | "book"
            | "camera"
            | "heart"
            | "star"
            | "mail"
            | "shop"
            | "forum"
            | "headphones"
            | "calendar"
            | "coffee"
            | "thread"
            | "doc"
            | "palette"
            | "paw"
    )
}

/// Guess an icon from a URL so a member can paste a link and get something
/// that looks intentional without choosing.
pub fn icon_for_url(url: &str) -> &'static str {
    let u = url.to_lowercase();
    let table: [(&str, &str); 26] = [
        ("bsky.app", "bsky"),
        ("github.com", "github"),
        ("youtube.com", "youtube"),
        ("youtu.be", "youtube"),
        ("twitch.tv", "twitch"),
        ("x.com", "x"),
        ("twitter.com", "x"),
        ("discord", "discord"),
        ("reddit.com", "reddit"),
        ("ko-fi.com", "kofi"),
        ("patreon.com", "patreon"),
        ("bandcamp.com", "bandcamp"),
        ("spotify.com", "spotify"),
        ("steampowered.com", "steam"),
        ("itch.io", "itch"),
        ("substack.com", "pen"),
        ("medium.com", "pen"),
        ("goodreads.com", "book"),
        ("archiveofourown", "book"),
        ("instagram.com", "camera"),
        ("tiktok.com", "music"),
        ("soundcloud.com", "headphones"),
        ("podcasts.apple", "mic"),
        ("open.spotify", "spotify"),
        ("rss", "rss"),
        ("forum", "forum"),
    ];
    for (needle, icon) in table {
        if u.contains(needle) {
            return icon;
        }
    }
    if u.contains("/feed") || u.ends_with(".json") {
        return "rss";
    }
    "link"
}

/// Gradient identicon: deterministic per handle, no upload, no request.
pub fn avatar(name: &str, url: Option<&str>, accent: &str, size: u32) -> String {
    if let Some(u) = url.map(str::trim).filter(|u| !u.is_empty()) {
        return format!(
            r#"<img class="av" src="{}" alt="" loading="lazy" width="{s}" height="{s}" style="width:{s}px;height:{s}px">"#,
            esc(u),
            s = size
        );
    }
    let accent2 = accent_shift(accent, 40);
    format!(
        r#"<span class="av av-gen" style="--accent:{a};--accent2:{b};width:{s}px;height:{s}px;font-size:{fs}px">{ini}</span>"#,
        a = esc(accent),
        b = esc(&accent2),
        s = size,
        fs = (size as f32 * 0.36).round() as u32,
        ini = esc(&initials(name))
    )
}

/// Second gradient stop: the accent rotated in hue, so every identicon has a
/// gradient without a second colour being stored.
fn accent_shift(hex: &str, deg: i32) -> String {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return "#1b1e34".to_string();
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(124) as f32 / 255.0;
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(92) as f32 / 255.0;
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(255) as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let mut hue = if (max - min).abs() < f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * (((g - b) / (max - min)) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / (max - min) + 2.0)
    } else {
        60.0 * ((r - g) / (max - min) + 4.0)
    };
    hue = (hue + deg as f32 + 360.0) % 360.0;
    let s = if l > 0.5 {
        (max - min) / (2.0 - max - min)
    } else {
        (max - min) / (max + min).max(0.0001)
    };
    let (l2, s2) = (0.42f32, s.min(0.9).max(0.25));
    let c = (1.0 - (2.0 * l2 - 1.0).abs()) * s2;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l2 - c / 2.0;
    let (r2, g2, b2) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    format!(
        "#{:02x}{:02x}{:02x}",
        ((r2 + m) * 255.0) as u8,
        ((g2 + m) * 255.0) as u8,
        ((b2 + m) * 255.0) as u8
    )
}

pub fn error_page(status: u16, title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title} — {SITE_NAME}</title>
<link rel="stylesheet" href="/assets/site.css">
</head><body class="errpage">
<main class="wrap errbox">
<p class="eyebrow">{status}</p>
<h1>{title}</h1>
<p class="muted">{body}</p>
<p><a class="btn" href="/">Back to the directory</a></p>
</main></body></html>"#,
        title = esc(title),
        body = esc(body),
    )
}
