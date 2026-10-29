//! View structs and the templates that render them. Everything a template
//! shows is pre-computed here: a template that calls a formatter is a template
//! that can be given the wrong one.

use askama::Template;
use askama_web::WebTemplate;
use chrono::{DateTime, Utc};

use crate::html::{avatar, icon_svg};
use crate::models::{LinkRow, MemberStat, User, VerticalRow};
use crate::text::{first_line, pretty_url};

pub const SITE_NAME: &str = "Orbital";
pub const TAGLINE: &str = "A directory of people, sorted by what they make";

pub struct NavItem {
    pub slug: String,
    pub label: String,
    pub icon: String,
}

pub struct NavUser {
    pub display_name: String,
}

pub struct Layout {
    pub base: String,
    pub nsfw: bool,
    pub site_name: String,
    pub tagline: String,
    pub nav: Vec<NavItem>,
    pub me: Option<NavUser>,
}

impl Layout {
    pub fn new(
        base: &str,
        nsfw: bool,
        verticals: &[VerticalRow],
        me: Option<&User>,
    ) -> Self {
        Self {
            base: base.to_string(),
            nsfw,
            site_name: SITE_NAME.to_string(),
            tagline: TAGLINE.to_string(),
            nav: verticals
                .iter()
                .filter(|v| !v.nsfw)
                .map(|v| NavItem {
                    slug: v.slug.clone(),
                    label: v.label.clone(),
                    icon: icon_svg(&v.icon),
                })
                .collect(),
            me: me.map(|u| NavUser {
                display_name: u.display_name.clone(),
            }),
        }
    }
}

#[derive(Clone)]
pub struct MemberCard {
    pub url: String,
    pub handle: String,
    pub display_name: String,
    pub tagline: String,
    pub avatar: String,
    pub avatar_mini: String,
    pub accent: String,
    pub tags: Vec<String>,
    pub rank: usize,
    pub is_new: bool,
    pub verified: bool,
    pub role: String,
    pub stat: String,
    pub stat_value: String,
    pub stat_label: String,
    pub why: String,
}

pub fn fmt_count(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}m", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn month_year(t: DateTime<Utc>) -> String {
    t.format("%b %Y").to_string()
}

pub fn is_new(t: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    (now - t).num_days() < 14
}

pub fn member_card(
    m: &MemberStat,
    base: &str,
    rank: usize,
    mode: StatMode,
    now: DateTime<Utc>,
) -> MemberCard {
    let url = format!("{base}/@{}", m.handle);
    let (value, label, stat) = match mode {
        StatMode::Top => (
            fmt_count(m.visitors),
            "visitors".to_string(),
            format!("{} visitors", fmt_count(m.visitors)),
        ),
        StatMode::Trending => (
            fmt_count(m.visitors_7d),
            "this week".to_string(),
            format!("{} this week", fmt_count(m.visitors_7d)),
        ),
        StatMode::New => (
            fmt_count(m.visitors),
            "visitors".to_string(),
            format!("joined {}", short_age(m.created_at, now)),
        ),
        StatMode::Similar => (
            fmt_count(m.visitors),
            "visitors".to_string(),
            String::new(),
        ),
    };
    MemberCard {
        url,
        handle: m.handle.clone(),
        display_name: m.display_name.clone(),
        tagline: first_line(&m.bio, 90),
        avatar: avatar(&m.display_name, m.avatar_url.as_deref(), &m.accent, 44),
        avatar_mini: avatar(&m.display_name, m.avatar_url.as_deref(), &m.accent, 22),
        accent: m.accent.clone(),
        tags: m.tags.iter().take(4).cloned().collect(),
        rank,
        is_new: is_new(m.created_at, now),
        verified: false,
        role: m.role.clone(),
        stat,
        stat_value: value,
        stat_label: label,
        why: String::new(),
    }
}

pub fn short_age(created: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let days = (now - created).num_days();
    match days {
        d if d <= 0 => "today".to_string(),
        1 => "yesterday".to_string(),
        d if d < 30 => format!("{d} days ago"),
        d if d < 365 => format!("{} months ago", d / 30),
        d => format!("{} years ago", d / 365),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum StatMode {
    Top,
    Trending,
    New,
    Similar,
}

// ------------------------------------------------------------------ home

pub struct VerticalCard {
    pub slug: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub members: i64,
    pub accent: String,
    pub top: Vec<MemberCard>,
}

pub struct SmallVertical {
    pub slug: String,
    pub label: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "home.html")]
pub struct HomeTemplate {
    pub layout: Layout,
    pub verticals: Vec<VerticalCard>,
    pub other: Vec<SmallVertical>,
    pub trending: Vec<MemberCard>,
    pub new_members: Vec<MemberCard>,
    pub picks: Vec<MemberCard>,
    pub total_members: i64,
    pub total_clicks: i64,
    pub quick: Vec<(String, String)>,
    pub q: String,
}

// ------------------------------------------------------------------ vertical

pub struct VerticalHead {
    pub slug: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub members: i64,
    pub nsfw: bool,
    pub accent: String,
    pub visitors_total: i64,
}

pub struct Tab {
    pub label: String,
    pub href: String,
    pub active: bool,
    pub count: i64,
}

pub struct TagCount {
    pub slug: String,
    pub count: i64,
    pub href: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "vertical.html")]
pub struct VerticalTemplate {
    pub layout: Layout,
    pub v: VerticalHead,
    pub tabs: Vec<Tab>,
    pub tab: String,
    pub rows: Vec<MemberCard>,
    pub tags: Vec<TagCount>,
    pub active_tag: String,
    pub clear_tag: String,
    pub embed_snippet: String,
}

// ------------------------------------------------------------------ profile

pub struct Badge {
    pub slug: String,
    pub label: String,
    pub icon: String,
    pub href: String,
    pub role: String,
}

pub struct LinkCard {
    pub id: String,
    pub href: String,
    pub label: String,
    pub host: String,
    pub icon: String,
    pub clicks: i64,
    pub clicks_label: String,
}

pub struct Ring {
    pub prev_href: String,
    pub prev_name: String,
    pub center: String,
    pub center_href: String,
    pub next_href: String,
    pub next_name: String,
}

pub struct ProfileView {
    pub display_name: String,
    pub handle: String,
    pub custom_domain: String,
    pub verified: bool,
    pub bsky_url: String,
    pub avatar: String,
    pub accent: String,
    pub verticals: Vec<Badge>,
    pub trust_label: String,
    pub bio: String,
    pub tags: Vec<String>,
    pub links: Vec<LinkCard>,
    pub stat_visitors: String,
    pub stat_clicks: String,
    pub stat_7d: String,
    pub stat_joined: String,
    pub is_self: bool,
    pub nsfw: bool,
    pub jsonld: String,
    pub canonical: String,
    pub meta_description: String,
    pub rings: Vec<Ring>,
    pub similar: Vec<MemberCard>,
    pub first_name: String,
    pub total_members: i64,
}

#[derive(Template, WebTemplate)]
#[template(path = "profile.html")]
pub struct ProfileTemplate {
    pub layout: Layout,
    pub p: ProfileView,
}

// ------------------------------------------------------------------ search

#[derive(Template, WebTemplate)]
#[template(path = "search.html")]
pub struct SearchTemplate {
    pub layout: Layout,
    pub q: String,
    pub members: Vec<MemberCard>,
    pub tags: Vec<TagCount>,
    pub quick: Vec<(String, String)>,
}

// ------------------------------------------------------------------ embed

#[derive(Template, WebTemplate)]
#[template(path = "embed.html")]
pub struct EmbedTemplate {
    pub title: String,
    pub subtitle: String,
    pub members: Vec<MemberCard>,
    pub join_url: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "about.html")]
pub struct AboutTemplate {
    pub layout: Layout,
}

#[derive(Template, WebTemplate)]
#[template(path = "report.html")]
pub struct ReportTemplate {
    pub layout: Layout,
    pub handle: String,
    pub vertical: String,
    pub done: bool,
    pub error: String,
}

/// Tracked outbound URL: the click is what feeds every leaderboard, so a
/// profile link never points straight at the third party.
pub fn tracked_link(base: &str, link: &LinkRow, surface: &str, ctx: Option<&str>, pos: usize) -> String {
    let mut url = format!(
        "{base}/l/{}?s={}&p={}",
        link.id,
        urlencode(surface),
        pos
    );
    if let Some(ctx) = ctx {
        url.push_str(&format!("&c={}", urlencode(ctx)));
    }
    url
}

pub fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32 as u8)
            }
        })
        .collect()
}

pub fn link_card(base: &str, link: &LinkRow, surface: &str, ctx: Option<&str>, pos: usize) -> LinkCard {
    LinkCard {
        id: link.id.to_string(),
        href: tracked_link(base, link, surface, ctx, pos),
        label: link.label.clone(),
        host: pretty_url(&link.url),
        icon: icon_svg(&link.icon),
        clicks: link.click_count,
        clicks_label: fmt_count(link.click_count),
    }
}

pub fn trust_label(level: i16) -> String {
    match level {
        0 => "new".to_string(),
        1 => "member".to_string(),
        2 => "trusted".to_string(),
        _ => "curator".to_string(),
    }
}

/// JSON-LD for a profile: the thing a search engine reads to know that this
/// page is a person and where else they exist.
pub fn profile_jsonld(
    name: &str,
    handle: &str,
    url: &str,
    bio: &str,
    avatar_url: Option<&str>,
    same_as: &[String],
    tags: &[String],
) -> String {
    let mut doc = serde_json::json!({
        "@context": "https://schema.org",
        "@type": "ProfilePage",
        "mainEntity": {
            "@type": "Person",
            "name": name,
            "alternateName": format!("@{handle}"),
            "url": url,
            "description": bio,
            "knowsAbout": tags,
        }
    });
    if let Some(a) = avatar_url.filter(|a| !a.is_empty()) {
        doc["mainEntity"]["image"] = serde_json::Value::String(a.to_string());
    }
    if !same_as.is_empty() {
        doc["mainEntity"]["sameAs"] = serde_json::Value::Array(
            same_as
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        );
    }
    serde_json::to_string(&doc).unwrap_or_else(|_| "{}".to_string())
}

/// "3 shared tags" or "same vertical" — a reason, so the sidebar is not a
/// mystery list.
pub fn similarity_reason(candidate: &MemberStat, viewer_tags: &[String], ctx: Option<&str>) -> String {
    let shared = candidate
        .tags
        .iter()
        .filter(|t| viewer_tags.contains(t))
        .count();
    if shared > 0 {
        return format!("{shared} shared tag{}", if shared == 1 { "" } else { "s" });
    }
    if let Some(ctx) = ctx {
        if candidate.verticals.first().map(|s| s.as_str()) == Some(ctx) {
            return "same vertical".to_string();
        }
        if candidate.verticals.iter().any(|v| v == ctx) {
            return "shares a vertical".to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_short_and_never_wrong_by_an_order_of_magnitude() {
        assert_eq!(fmt_count(0), "0");
        assert_eq!(fmt_count(999), "999");
        assert_eq!(fmt_count(1000), "1.0k");
        assert_eq!(fmt_count(1234), "1.2k");
        assert_eq!(fmt_count(2_000_000), "2.0m");
    }

    #[test]
    fn urlencode_keeps_slugs_readable() {
        assert_eq!(urlencode("indie-web"), "indie-web");
        assert_eq!(urlencode("a b"), "a%20b");
    }
}
