//! Public, server-rendered pages. Everything a crawler or a visitor sees
//! without signing in, rendered as HTML by the Rust process.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Form, Router};
use chrono::Utc;
use serde::Deserialize;

use crate::auth::{Ctx, PageUser, VisitorId};
use crate::error::{AppError, PageError, PageResult};
use crate::html::{avatar, icon_svg, SITE_NAME};
use crate::models::{MemberStat, User};
use crate::ranking;
use crate::render::*;
use crate::store;
use crate::text::{first_line, normalise_handle};
use crate::AppState;

const AGE_COOKIE: &str = "orbital_age";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/about", get(about))
        .route("/v/{slug}", get(vertical))
        .route("/v/{slug}/embed", get(embed))
        .route("/v/{slug}/feed.json", get(vertical_feed))
        .route("/@{handle}", get(profile))
        .route("/u/{handle}", get(profile_redirect))
        .route("/search", get(search))
        .route("/feed.json", get(feed_json))
        .route("/feed.xml", get(feed_xml))
        .route("/sitemap.xml", get(sitemap))
        .route("/robots.txt", get(robots))
        .route("/gate", get(gate).post(gate_post))
        .route("/report", get(report).post(report_post))
}

pub(crate) fn visible(m: &MemberStat, is_nsfw: bool) -> bool {
    is_nsfw || !m.nsfw_enabled
}

pub(crate) fn card(m: &MemberStat, base: &str, rank: usize, mode: StatMode, now: chrono::DateTime<Utc>) -> MemberCard {
    let mut c = member_card(m, base, rank, mode, now);
    c.verified = m.bsky_handle.is_some();
    c
}

fn layout_for(
    ctx: &Ctx,
    verticals: &[crate::models::VerticalRow],
    me: Option<&User>,
) -> Layout {
    Layout::new(&ctx.base, ctx.is_nsfw, verticals, me)
}

// ------------------------------------------------------------------ home

#[derive(Deserialize)]
pub struct HomeQuery {
    #[serde(default)]
    pub q: String,
}

pub async fn home(
    State(st): State<AppState>,
    ctx: Ctx,
    PageUser(user): PageUser,
    Query(q): Query<HomeQuery>,
) -> PageResult<HomeTemplate> {
    let now = Utc::now();
    let verticals = store::verticals(&st.pool, ctx.is_nsfw).await?;
    let mut cards: Vec<VerticalCard> = Vec::new();
    let mut other: Vec<SmallVertical> = Vec::new();

    for v in &verticals {
        let members = store::vertical_members(&st.pool, &v.slug).await?;
        let members: Vec<MemberStat> = members
            .into_iter()
            .filter(|m| visible(m, ctx.is_nsfw))
            .collect();
        if members.is_empty() {
            other.push(SmallVertical {
                slug: v.slug.clone(),
                label: v.label.clone(),
            });
            continue;
        }
        let ranked = ranking::rank_top(members);
        let top = ranked
            .iter()
            .take(3)
            .enumerate()
            .map(|(i, m)| card(m, &ctx.base, i + 1, StatMode::Top, now))
            .collect();
        cards.push(VerticalCard {
            slug: v.slug.clone(),
            label: v.label.clone(),
            description: v.description.clone(),
            icon: icon_svg(&v.icon),
            members: ranked.len() as i64,
            accent: crate::text::accent_for(&v.slug),
            top,
        });
    }
    // A vertical with an adult flag never appears on the SFW host, and the SFW
    // verticals never appear on the adult host: the two directories are separate
    // products that happen to share a binary.
    if ctx.is_nsfw {
        cards.retain(|c| {
            verticals
                .iter()
                .any(|v| v.slug == c.slug && v.nsfw)
        });
        other.retain(|o| verticals.iter().any(|v| v.slug == o.slug && v.nsfw));
    }

    let all = store::all_members(&st.pool, ctx.is_nsfw).await?;
    let all: Vec<MemberStat> = all
        .into_iter()
        .filter(|m| visible(m, ctx.is_nsfw))
        .collect();
    let trending: Vec<MemberCard> = ranking::rank_trending(all.clone(), now)
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, m)| card(m, &ctx.base, i + 1, StatMode::Trending, now))
        .collect();
    let new_members: Vec<MemberCard> = ranking::rank_new(all.clone())
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, m)| card(m, &ctx.base, i + 1, StatMode::New, now))
        .collect();
    let picks: Vec<MemberCard> = store::curated_any(&st.pool, ctx.is_nsfw, 6)
        .await?
        .iter()
        .enumerate()
        .map(|(i, m)| card(m, &ctx.base, i + 1, StatMode::Top, now))
        .collect();

    let (total_members, total_clicks) = store::totals(&st.pool, ctx.is_nsfw).await?;
    let quick = cards
        .iter()
        .take(3)
        .map(|c| (c.slug.clone(), c.label.clone()))
        .collect();

    Ok(HomeTemplate {
        layout: layout_for(&ctx, &verticals, user.as_ref()),
        verticals: cards,
        other,
        trending,
        new_members,
        picks,
        total_members,
        total_clicks: if ctx.is_nsfw { 0 } else { total_clicks },
        quick,
        q: q.q,
    })
}

// ------------------------------------------------------------------ vertical

#[derive(Deserialize)]
pub struct VerticalQuery {
    #[serde(default)]
    pub tab: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
}

pub async fn vertical(
    State(st): State<AppState>,
    ctx: Ctx,
    PageUser(user): PageUser,
    Path(slug): Path<String>,
    Query(q): Query<VerticalQuery>,
) -> PageResult<VerticalTemplate> {
    let now = Utc::now();
    let Some(v) = store::vertical_by_slug(&st.pool, &slug).await? else {
        return Err(PageError(AppError::NotFound));
    };
    if v.nsfw != ctx.is_nsfw {
        return Err(PageError(AppError::NotFound));
    }
    let verticals = store::verticals(&st.pool, ctx.is_nsfw).await?;
    let members: Vec<MemberStat> = store::vertical_members(&st.pool, &v.slug)
        .await?
        .into_iter()
        .filter(|m| visible(m, ctx.is_nsfw))
        .collect();

    let tab = q
        .tab
        .clone()
        .unwrap_or_else(|| "top".to_string())
        .to_lowercase();
    let tab = match tab.as_str() {
        "trending" | "new" | "curated" => tab,
        _ => "top".to_string(),
    };
    let active_tag = q.tag.clone().unwrap_or_default();

    let filtered: Vec<MemberStat> = if active_tag.is_empty() {
        members.clone()
    } else {
        members
            .iter()
            .filter(|m| m.tags.iter().any(|t| t == &active_tag))
            .cloned()
            .collect()
    };

    let ranked: Vec<MemberStat> = match tab.as_str() {
        "trending" => ranking::rank_trending(filtered.clone(), now),
        "new" => ranking::rank_new(filtered.clone()),
        "curated" => {
            let curated = store::curated(&st.pool, &v.slug).await?;
            curated
                .into_iter()
                .filter(|m| visible(m, ctx.is_nsfw))
                .collect()
        }
        _ => ranking::rank_top(filtered.clone()),
    };
    let mode = match tab.as_str() {
        "trending" => StatMode::Trending,
        "new" => StatMode::New,
        _ => StatMode::Top,
    };
    let rows: Vec<MemberCard> = ranked
        .iter()
        .take(100)
        .enumerate()
        .map(|(i, m)| card(m, &ctx.base, i + 1, mode, now))
        .collect();

    let visitors_total: i64 = members.iter().map(|m| m.visitors).sum();
    let mut tag_counts: std::collections::BTreeMap<String, i64> = Default::default();
    for m in &members {
        for t in &m.tags {
            *tag_counts.entry(t.clone()).or_default() += 1;
        }
    }
    let mut tags: Vec<TagCount> = tag_counts
        .into_iter()
        .filter(|(t, n)| *n >= 1 && t != &active_tag)
        .map(|(t, n)| TagCount {
            href: format!("/v/{}?tab={}&tag={}", v.slug, tab, urlencode(&t)),
            slug: t,
            count: n,
        })
        .take(18)
        .collect();
    tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.slug.cmp(&b.slug)));

    let tabs = vec![
        Tab {
            label: "Top".into(),
            href: format!("/v/{}?tab=top", v.slug),
            active: tab == "top",
            count: members.len() as i64,
        },
        Tab {
            label: "Trending".into(),
            href: format!("/v/{}?tab=trending", v.slug),
            active: tab == "trending",
            count: ranking::rank_trending(members.clone(), now).len() as i64,
        },
        Tab {
            label: "New".into(),
            href: format!("/v/{}?tab=new", v.slug),
            active: tab == "new",
            count: members.len() as i64,
        },
        Tab {
            label: "Curated".into(),
            href: format!("/v/{}?tab=curated", v.slug),
            active: tab == "curated",
            count: store::curated(&st.pool, &v.slug).await?.len() as i64,
        },
    ];

    let embed_snippet = format!(
        r#"<iframe src="{}/v/{}/embed?view={}&count=5" width="300" height="360" frameborder="0" style="border:0;border-radius:12px" title="{} leaderboard"></iframe>"#,
        ctx.base, v.slug, tab, v.label
    );

    Ok(VerticalTemplate {
        layout: layout_for(&ctx, &verticals, user.as_ref()),
        v: VerticalHead {
            slug: v.slug.clone(),
            label: v.label.clone(),
            description: v.description.clone(),
            icon: icon_svg(&v.icon),
            members: members.len() as i64,
            nsfw: v.nsfw,
            accent: crate::text::accent_for(&v.slug),
            visitors_total,
        },
        tabs,
        tab: tab.clone(),
        rows,
        tags,
        active_tag: active_tag.clone(),
        clear_tag: format!("/v/{}?tab={}", v.slug, tab),
        embed_snippet,
    })
}

// ------------------------------------------------------------------ profile

pub async fn profile_redirect(Path(handle): Path<String>) -> Redirect {
    Redirect::permanent(&format!("/@{handle}"))
}

pub async fn profile(
    State(st): State<AppState>,
    ctx: Ctx,
    PageUser(user): PageUser,
    VisitorId(vid): VisitorId,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> PageResult<Response> {
    let now = Utc::now();
    let Ok(handle) = normalise_handle(&handle) else {
        return Err(PageError(AppError::NotFound));
    };
    let Some(owner) = store::user_by_handle(&st.pool, &handle).await? else {
        return Err(PageError(AppError::NotFound));
    };
    if owner.hidden {
        return Err(PageError(AppError::NotFound));
    }

    let pairs = store::verticals_of(&st.pool, owner.id).await?;
    let primary = pairs
        .iter()
        .find(|(_, role)| role == "primary")
        .map(|(s, _)| s.clone());
    let primary_nsfw = match &primary {
        Some(slug) => store::vertical_by_slug(&st.pool, slug)
            .await?
            .map(|v| v.nsfw)
            .unwrap_or(false),
        None => false,
    };

    // Segregation, both directions and before anything renders.
    if ctx.is_nsfw {
        if !primary_nsfw {
            return Err(PageError(AppError::NotFound));
        }
        if owner.trust_level < 1 {
            return Err(PageError(AppError::NotFound));
        }
        if !age_ok(&headers) {
            let verticals = store::verticals(&st.pool, true).await?;
            return Ok(GateTemplate {
                layout: layout_for(&ctx, &verticals, user.as_ref()),
                next: format!("/@{handle}"),
                sfw_url: st.cfg.public_base.clone(),
            }
            .into_response());
        }
    } else if primary_nsfw || owner.nsfw_enabled {
        return Err(PageError(AppError::NotFound));
    }

    let ctx_vertical = if ctx.is_nsfw {
        Some("nsfw".to_string())
    } else {
        primary.clone()
    };
    let is_self = user.as_ref().map(|u| u.id) == Some(owner.id);

    // A view is a fact about the visitor's day, not about the request: the
    // INSERT dedupes per (member, session, day) and the owner does not count.
    if !is_self {
        let hash = crate::auth::session_hash(&st.cfg.salt, &vid);
        let _ = store::record_view(
            &st.pool,
            owner.id,
            ctx_vertical.as_deref(),
            &hash,
        )
        .await;
    }

    let links = store::links_for(
        &st.pool,
        owner.id,
        ctx_vertical.as_deref(),
        ctx.is_nsfw,
    )
    .await?;
    let link_cards: Vec<LinkCard> = links
        .iter()
        .enumerate()
        .map(|(i, l)| link_card(&ctx.base, l, "profile", ctx_vertical.as_deref(), i + 1))
        .collect();

    let tags = store::tags_of(&st.pool, owner.id).await?;
    let stat = store::member_stat(&st.pool, owner.id, ctx_vertical.as_deref())
        .await?
        .ok_or(PageError(AppError::NotFound))?;

    // An adult vertical badge never renders on the SFW host.
    let badges: Vec<Badge> = pairs
        .iter()
        .filter(|(slug, _)| ctx.is_nsfw || slug != "nsfw")
        .map(|(slug, role)| Badge {
            slug: slug.clone(),
            label: pretty_vertical(slug),
            icon: icon_svg(icon_for_slug(slug)),
            href: format!("/v/{slug}"),
            role: role.clone(),
        })
        .collect();

    // Similar profiles: same formula as the spec, computed here from tag sets.
    let all = store::all_members(&st.pool, ctx.is_nsfw).await?;
    let owner_tags = store::tags_of(&st.pool, owner.id).await?;
    let viewer_tags = match &user {
        Some(u) if u.id != owner.id => store::tags_of(&st.pool, u.id).await?,
        _ => owner_tags.clone(),
    };
    let mut scored: Vec<(f64, &MemberStat)> = all
        .iter()
        .filter(|m| m.id != owner.id && visible(m, ctx.is_nsfw))
        .map(|m| {
            (
                ranking::similarity(&viewer_tags, &owner_tags, m, primary.as_deref(), now),
                m,
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let similar: Vec<MemberCard> = scored
        .iter()
        .filter(|(s, _)| *s > 0.05)
        .take(5)
        .map(|(_, m)| {
            let mut c = card(m, &ctx.base, 0, StatMode::Similar, now);
            c.why = similarity_reason(m, &owner_tags, primary.as_deref());
            c
        })
        .collect();

    // Rings: the primary vertical's ring, then the global one.
    let mut rings: Vec<Ring> = Vec::new();
    let hub = store::owner_id(&st.pool).await?;
    let hub_tags = match hub {
        Some(id) => store::tags_of(&st.pool, id).await?,
        None => Vec::new(),
    };
    let day = now.timestamp() / 86_400;
    if let Some(p) = &primary {
        let members: Vec<MemberStat> = store::vertical_members(&st.pool, p)
            .await?
            .into_iter()
            .filter(|m| visible(m, ctx.is_nsfw))
            .collect();
        let order = ranking::ring(&members, &hub_tags, &[p.clone()], day);
        let (prev, next) = ranking::neighbours(&order, owner.id);
        rings.push(Ring {
            prev_href: prev.map(|m| format!("{}/@{}", ctx.base, m.handle)).unwrap_or_default(),
            prev_name: prev.map(|m| m.display_name.clone()).unwrap_or_default(),
            center: format!("{} directory", pretty_vertical(p)),
            center_href: format!("/v/{p}"),
            next_href: next.map(|m| format!("{}/@{}", ctx.base, m.handle)).unwrap_or_default(),
            next_name: next.map(|m| m.display_name.clone()).unwrap_or_default(),
        });
    }
    let all_visible: Vec<MemberStat> = all
        .iter()
        .filter(|m| visible(m, ctx.is_nsfw))
        .cloned()
        .collect();
    let order = ranking::ring(&all_visible, &hub_tags, &[], day);
    let (prev, next) = ranking::neighbours(&order, owner.id);
    rings.push(Ring {
        prev_href: prev.map(|m| format!("{}/@{}", ctx.base, m.handle)).unwrap_or_default(),
        prev_name: prev.map(|m| m.display_name.clone()).unwrap_or_default(),
        center: "Everyone".to_string(),
        center_href: "/".to_string(),
        next_href: next.map(|m| format!("{}/@{}", ctx.base, m.handle)).unwrap_or_default(),
        next_name: next.map(|m| m.display_name.clone()).unwrap_or_default(),
    });

    let canonical = format!("{}/@{}", ctx.base, owner.handle);
    let same_as: Vec<String> = [
        owner
            .bsky_handle
            .as_ref()
            .map(|h| format!("https://bsky.app/profile/{h}")),
        None,
    ]
    .into_iter()
    .flatten()
    .collect();
    let jsonld = profile_jsonld(
        &owner.display_name,
        &owner.handle,
        &canonical,
        &first_line(&owner.bio, 300),
        owner.avatar_url.as_deref(),
        &same_as,
        &tags,
    );
    let meta_description = if owner.bio.trim().is_empty() {
        format!(
            "{} (@{}) on {} — {}",
            owner.display_name,
            owner.handle,
            SITE_NAME,
            primary.as_deref().map(pretty_vertical).unwrap_or_else(|| "the directory".to_string())
        )
    } else {
        first_line(&owner.bio, 160)
    };
    let total_members = store::member_count(&st.pool, ctx.is_nsfw).await?;

    let view = ProfileView {
        display_name: owner.display_name.clone(),
        handle: owner.handle.clone(),
        custom_domain: String::new(),
        verified: owner.bsky_handle.is_some(),
        bsky_url: owner
            .bsky_handle
            .as_ref()
            .map(|h| format!("https://bsky.app/profile/{h}"))
            .unwrap_or_default(),
        avatar: avatar(&owner.display_name, owner.avatar_url.as_deref(), &owner.accent, 96),
        accent: owner.accent.clone(),
        verticals: badges,
        trust_label: if owner.trust_level >= 2 {
            trust_label(owner.trust_level)
        } else {
            String::new()
        },
        bio: owner.bio.clone(),
        tags: tags.clone(),
        links: link_cards,
        stat_visitors: fmt_count(stat.visitors),
        stat_clicks: fmt_count(stat.clicks),
        stat_7d: fmt_count(stat.visitors_7d),
        stat_joined: month_year(owner.created_at),
        is_self,
        nsfw: ctx.is_nsfw,
        jsonld,
        canonical,
        meta_description,
        rings,
        similar,
        first_name: owner
            .display_name
            .split_whitespace()
            .next()
            .unwrap_or(&owner.display_name)
            .to_string(),
        total_members,
    };

    let verticals = store::verticals(&st.pool, ctx.is_nsfw).await?;
    Ok(ProfileTemplate {
        layout: layout_for(&ctx, &verticals, user.as_ref()),
        p: view,
    }
    .into_response())
}

fn pretty_vertical(slug: &str) -> String {
    match slug {
        "gaming" => "Video Games",
        "podcasters" => "Podcasters",
        "youtube" => "YouTubers",
        "music" => "Musicians",
        "art" => "Visual Artists",
        "dev" => "Developers",
        "writers" => "Writers",
        "vtubers" => "VTubers",
        "indie-web" => "Indie Web",
        "nsfw" => "Adult Creators",
        other => other,
    }
    .to_string()
}

fn icon_for_slug(slug: &str) -> &'static str {
    match slug {
        "gaming" => "game",
        "podcasters" => "mic",
        "youtube" => "video",
        "music" => "music",
        "art" => "brush",
        "dev" => "code",
        "writers" => "pen",
        "vtubers" => "mask",
        "indie-web" => "globe",
        "nsfw" => "flame",
        _ => "dot",
    }
}

pub(crate) fn age_ok(headers: &HeaderMap) -> bool {
    crate::auth::cookie_value(headers, AGE_COOKIE).as_deref() == Some("1")
}

// ------------------------------------------------------------------ search

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

pub async fn search(
    State(st): State<AppState>,
    ctx: Ctx,
    PageUser(user): PageUser,
    Query(q): Query<SearchQuery>,
) -> PageResult<SearchTemplate> {
    let now = Utc::now();
    let verticals = store::verticals(&st.pool, ctx.is_nsfw).await?;
    let query = q.q.trim().to_string();
    let (mut members, mut tags) = if query.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        store::search(&st.pool, &query, ctx.is_nsfw).await?
    };
    if !ctx.is_nsfw {
        members.retain(|m| !m.nsfw_enabled);
    }
    let cards: Vec<MemberCard> = members
        .iter()
        .enumerate()
        .map(|(i, m)| card(m, &ctx.base, i + 1, StatMode::Top, now))
        .collect();
    let tag_rows = tags
        .drain(..)
        .map(|(slug, count)| TagCount {
            href: format!("/search?q={}", urlencode(&slug)),
            slug,
            count,
        })
        .collect();
    let quick = verticals
        .iter()
        .filter(|v| !v.nsfw)
        .take(10)
        .map(|v| (v.slug.clone(), v.label.clone()))
        .collect();

    Ok(SearchTemplate {
        layout: layout_for(&ctx, &verticals, user.as_ref()),
        q: query,
        members: cards,
        tags: tag_rows,
        quick,
    })
}

// ------------------------------------------------------------------ embeds

#[derive(Deserialize)]
pub struct EmbedQuery {
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub count: Option<i64>,
}

pub async fn embed(
    State(st): State<AppState>,
    ctx: Ctx,
    Path(slug): Path<String>,
    Query(q): Query<EmbedQuery>,
) -> PageResult<EmbedTemplate> {
    let now = Utc::now();
    let Some(v) = store::vertical_by_slug(&st.pool, &slug).await? else {
        return Err(PageError(AppError::NotFound));
    };
    if v.nsfw {
        return Err(PageError(AppError::NotFound));
    }
    let count = q.count.unwrap_or(5).clamp(1, 20);
    let view = q.view.unwrap_or_else(|| "top".to_string());
    let members: Vec<MemberStat> = store::vertical_members(&st.pool, &v.slug)
        .await?
        .into_iter()
        .filter(|m| !m.nsfw_enabled)
        .collect();
    let (ranked, mode) = match view.as_str() {
        "trending" => (ranking::rank_trending(members, now), StatMode::Trending),
        "new" => (ranking::rank_new(members), StatMode::New),
        _ => (ranking::rank_top(members), StatMode::Top),
    };
    let cards: Vec<MemberCard> = ranked
        .iter()
        .take(count as usize)
        .enumerate()
        .map(|(i, m)| card(m, &ctx.base, i + 1, mode, now))
        .collect();
    Ok(EmbedTemplate {
        title: v.label.clone(),
        subtitle: match mode {
            StatMode::Trending => "trending this week".to_string(),
            StatMode::New => "newest members".to_string(),
            _ => "all time".to_string(),
        },
        members: cards,
        join_url: format!("{}/app/#/join", ctx.base),
    })
}

// ------------------------------------------------------------------ feeds

pub async fn feed_json(
    State(st): State<AppState>,
    ctx: Ctx,
) -> PageResult<Response> {
    let members = store::all_members(&st.pool, ctx.is_nsfw).await?;
    let members: Vec<MemberStat> = members
        .into_iter()
        .filter(|m| visible(m, ctx.is_nsfw))
        .collect();
    let items: Vec<serde_json::Value> = members
        .iter()
        .take(100)
        .map(|m| {
            serde_json::json!({
                "id": format!("{}/@{}", st.cfg.public_base, m.handle),
                "url": format!("{}/@{}", st.cfg.public_base, m.handle),
                "title": format!("{} (@{})", m.display_name, m.handle),
                "content_text": m.bio,
                "tags": m.tags,
                "date_modified": m.last_active_at.to_rfc3339(),
            })
        })
        .collect();
    let feed = serde_json::json!({
        "version": "https://jsonfeed.org/version/1.1",
        "title": format!("{SITE_NAME} — everybody"),
        "home_page_url": format!("{}/", st.cfg.public_base),
        "feed_url": format!("{}/feed.json", st.cfg.public_base),
        "description": crate::render::TAGLINE,
        "items": items,
    });
    Ok((
        [(header::CONTENT_TYPE, "application/feed+json; charset=utf-8")],
        serde_json::to_string_pretty(&feed).unwrap_or_else(|_| "{}".into()),
    )
        .into_response())
}

pub(crate) async fn vertical_feed(
    State(st): State<AppState>,
    ctx: Ctx,
    Path(slug): Path<String>,
) -> PageResult<Response> {
    let Some(v) = store::vertical_by_slug(&st.pool, &slug).await? else {
        return Err(PageError(AppError::NotFound));
    };
    if v.nsfw && !ctx.is_nsfw {
        return Err(PageError(AppError::NotFound));
    }
    let members: Vec<MemberStat> = store::vertical_members(&st.pool, &v.slug)
        .await?
        .into_iter()
        .filter(|m| visible(m, ctx.is_nsfw))
        .collect();
    let items: Vec<serde_json::Value> = members
        .iter()
        .take(100)
        .map(|m| {
            serde_json::json!({
                "id": format!("{}/@{}", st.cfg.public_base, m.handle),
                "url": format!("{}/@{}", st.cfg.public_base, m.handle),
                "title": format!("{} (@{})", m.display_name, m.handle),
                "content_text": m.bio,
                "tags": m.tags,
                "date_modified": m.last_active_at.to_rfc3339(),
            })
        })
        .collect();
    let feed = serde_json::json!({
        "version": "https://jsonfeed.org/version/1.1",
        "title": format!("{SITE_NAME} — {}", v.label),
        "home_page_url": format!("{}/v/{}", st.cfg.public_base, v.slug),
        "feed_url": format!("{}/v/{}/feed.json", st.cfg.public_base, v.slug),
        "description": v.description,
        "items": items,
    });
    Ok((
        [(header::CONTENT_TYPE, "application/feed+json; charset=utf-8")],
        serde_json::to_string_pretty(&feed).unwrap_or_else(|_| "{}".into()),
    )
        .into_response())
}

pub(crate) async fn feed_xml(State(st): State<AppState>, ctx: Ctx) -> PageResult<Response> {
    let members = store::all_members(&st.pool, ctx.is_nsfw).await?;
    let members: Vec<MemberStat> = members
        .into_iter()
        .filter(|m| visible(m, ctx.is_nsfw))
        .collect();
    let mut items = String::new();
    for m in members.iter().take(100) {
        let url = format!("{}/@{}", st.cfg.public_base, m.handle);
        items.push_str(&format!(
            "<item><title>{} (@{})</title><link>{}</link><guid>{}</guid><description>{}</description><pubDate>{}</pubDate></item>",
            crate::text::esc(&m.display_name),
            crate::text::esc(&m.handle),
            crate::text::esc(&url),
            crate::text::esc(&url),
            crate::text::esc(&m.bio),
            m.last_active_at.to_rfc2822()
        ));
    }
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0"><channel>
<title>{SITE_NAME}</title>
<link>{base}/</link>
<description>{desc}</description>
<language>en</language>
{items}
</channel></rss>"#,
        base = st.cfg.public_base,
        desc = crate::text::esc(crate::render::TAGLINE),
    );
    Ok((
        [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
        xml,
    )
        .into_response())
}

pub(crate) async fn sitemap(State(st): State<AppState>, ctx: Ctx) -> PageResult<Response> {
    // The adult host has no sitemap: it is noindex, and a sitemap here would be
    // a list of everything we promised not to publish.
    let mut body = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<urlset xmlns="http://www.w3.org/1999/xhtml/sitemap/0.9">"#,
    );
    body = body.replace("http://www.w3.org/1999/xhtml/sitemap/0.9", "http://www.sitemaps.org/schemas/sitemap/0.9");
    if !ctx.is_nsfw {
        let base = &st.cfg.public_base;
        body.push_str(&format!(
            "<url><loc>{base}/</loc><changefreq>hourly</changefreq><priority>1.0</priority></url>"
        ));
        body.push_str(&format!(
            "<url><loc>{base}/about</loc><priority>0.5</priority></url>"
        ));
        for v in store::verticals(&st.pool, false).await? {
            let members = store::vertical_members(&st.pool, &v.slug).await?;
            if members.is_empty() {
                continue;
            }
            body.push_str(&format!(
                "<url><loc>{base}/v/{}</loc><changefreq>hourly</changefreq><priority>0.9</priority></url>",
                v.slug
            ));
        }
        for (handle, updated) in store::sitemap_entries(&st.pool, false).await? {
            body.push_str(&format!(
                "<url><loc>{base}/@{}</loc><lastmod>{}</lastmod><priority>0.8</priority></url>",
                crate::text::esc(&handle),
                updated.format("%Y-%m-%d")
            ));
        }
    }
    body.push_str("</urlset>");
    Ok((
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body,
    )
        .into_response())
}

pub(crate) async fn robots(ctx: Ctx) -> Response {
    let body = if ctx.is_nsfw {
        "User-agent: *\nDisallow: /\n".to_string()
    } else {
        format!(
            "User-agent: *\nAllow: /\nDisallow: /app\nDisallow: /l/\nDisallow: /api/\nDisallow: /gate\n\nSitemap: {}/sitemap.xml\n",
            ctx.base
        )
    };
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

// ------------------------------------------------------------------ gate

#[derive(Deserialize)]
pub struct GateQuery {
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn gate(
    State(st): State<AppState>,
    ctx: Ctx,
    Query(q): Query<GateQuery>,
) -> PageResult<GateTemplate> {
    // Only a local path is a legal destination: an open redirect on the age
    // gate is how a phishing link borrows this domain.
    let next = q
        .next
        .filter(|n| n.starts_with('/') && !n.starts_with("//"))
        .unwrap_or_else(|| "/".to_string());
    Ok(GateTemplate {
        layout: Layout::new(&ctx.base, true, &[], None),
        next,
        sfw_url: st.cfg.public_base.clone(),
    })
}

#[derive(Deserialize)]
pub struct GateForm {
    #[serde(default)]
    pub next: String,
    #[serde(default)]
    pub ok: Option<String>,
}

pub async fn gate_post(ctx: Ctx, Form(form): Form<GateForm>) -> Response {
    let next = if form.next.starts_with('/') && !form.next.starts_with("//") {
        form.next
    } else {
        "/".to_string()
    };
    let secure = if ctx.base.starts_with("https") { "; Secure" } else { "" };
    let cookie = HeaderValue::from_str(&format!(
        "{AGE_COOKIE}=1; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax{secure}"
    ))
    .expect("cookie is ascii");
    let mut res = Redirect::to(&next).into_response();
    res.headers_mut().append(header::SET_COOKIE, cookie);
    res
}

// ------------------------------------------------------------------ about + report

pub async fn about(
    State(st): State<AppState>,
    ctx: Ctx,
    PageUser(user): PageUser,
) -> PageResult<AboutTemplate> {
    let verticals = store::verticals(&st.pool, false).await?;
    Ok(AboutTemplate {
        layout: layout_for(&ctx, &verticals, user.as_ref()),
    })
}

pub async fn report(
    State(st): State<AppState>,
    ctx: Ctx,
    PageUser(user): PageUser,
    Query(q): Query<ReportQuery>,
) -> PageResult<ReportTemplate> {
    let verticals = store::verticals(&st.pool, ctx.is_nsfw).await?;
    Ok(ReportTemplate {
        layout: layout_for(&ctx, &verticals, user.as_ref()),
        handle: q.u.clone().unwrap_or_default(),
        vertical: q.v.clone().unwrap_or_default(),
        done: false,
        error: String::new(),
    })
}

#[derive(Deserialize)]
pub struct ReportQuery {
    #[serde(default)]
    pub u: Option<String>,
    #[serde(default)]
    pub v: Option<String>,
}

#[derive(Deserialize)]
pub struct ReportForm {
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub vertical: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub detail: String,
}

pub async fn report_post(
    State(st): State<AppState>,
    ctx: Ctx,
    PageUser(user): PageUser,
    Form(form): Form<ReportForm>,
) -> PageResult<ReportTemplate> {
    let verticals = store::verticals(&st.pool, ctx.is_nsfw).await?;
    let handle = normalise_handle(&form.handle).unwrap_or_default();
    let target = store::user_by_handle(&st.pool, &handle).await?;
    let Some(target) = target else {
        return Ok(ReportTemplate {
            layout: layout_for(&ctx, &verticals, user.as_ref()),
            handle: form.handle.clone(),
            vertical: form.vertical.clone(),
            done: false,
            error: "No profile with that handle.".to_string(),
        });
    };
    let reason = if form.reason.trim().is_empty() {
        "other".to_string()
    } else {
        crate::text::truncate(&form.reason, 60)
    };
    let vertical = if form.vertical.trim().is_empty() {
        None
    } else {
        Some(form.vertical.trim().to_string())
    };
    store::create_flag(
        &st.pool,
        user.as_ref().map(|u| u.id),
        target.id,
        vertical.as_deref(),
        &reason,
        &crate::text::truncate(&form.detail, 2000),
    )
    .await?;
    Ok(ReportTemplate {
        layout: layout_for(&ctx, &verticals, user.as_ref()),
        handle,
        vertical: String::new(),
        done: true,
        error: String::new(),
    })
}

// ------------------------------------------------------------------ helpers

