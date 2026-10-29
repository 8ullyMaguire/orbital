//! Text normalisation and validation. One place, because these rules are also
//! CHECK constraints in the schema and the two must not drift.

use crate::error::AppError;
use url::Url;

pub fn normalise_handle(raw: &str) -> Result<String, AppError> {
    let h = raw.trim().trim_start_matches('@').to_lowercase();
    if h.len() < 2 || h.len() > 30 {
        return Err(AppError::Validation(
            "a handle is 2–30 characters".to_string(),
        ));
    }
    if !h
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    {
        return Err(AppError::Validation(
            "handles use letters, numbers, dots and dashes".to_string(),
        ));
    }
    if !h.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::Validation(
            "a handle starts with a letter or number".to_string(),
        ));
    }
    Ok(h)
}

/// Tags people type differently and mean the same thing by.
///
/// The tuner takes free text, so `game-dev`, `game dev` and `gamedev` would otherwise be three
/// lenses that never match each other. This is deliberately a small hand-written list rather than a
/// fuzzy matcher: a wrong merge is worse than a missed one, and every entry here is a spelling
/// variant, not a judgement about what a tag means.
const TAG_ALIASES: &[(&str, &str)] = &[
    ("gamedev", "game-dev"),
    ("game-development", "game-dev"),
    ("indie-game-dev", "game-dev"),
    ("indiegames", "indie-games"),
    ("pixelart", "pixel-art"),
    ("synth-wave", "synthwave"),
    ("selfhosted", "self-hosting"),
    ("opensource", "open-source"),
    ("open-source-software", "open-source"),
    ("at-proto", "atproto"),
    ("bluesky-social", "bluesky"),
    ("b-sky", "bluesky"),
    ("ai-art", "ai-art"),
    ("music-production", "music-production"),
];

/// The canonical form of an already-normalised tag, or the tag itself.
pub fn canonical_tag(tag: &str) -> String {
    // Collapse "a--b" and trim stray hyphens before the alias table sees the tag.
    let mut collapsed = String::with_capacity(tag.len());
    let mut prev_dash = false;
    for c in tag.chars() {
        if c == '-' {
            if !prev_dash {
                collapsed.push(c);
            }
            prev_dash = true;
        } else {
            collapsed.push(c);
            prev_dash = false;
        }
    }
    let trimmed = collapsed.trim_matches('-');
    TAG_ALIASES
        .iter()
        .find(|(from, _)| *from == trimmed)
        .map(|(_, to)| to.to_string())
        .unwrap_or_else(|| trimmed.to_string())
}

pub fn normalise_tag(raw: &str) -> Result<String, AppError> {
    let t = raw
        .trim()
        .trim_start_matches('#')
        .to_lowercase()
        .replace([' ', '_'], "-")
        // "-rust-" is a typo, not a different tag: trim the edges before validating the first char.
        .trim_matches('-')
        .to_string();
    if t.is_empty() || t.len() > 30 {
        return Err(AppError::Validation(
            "a tag is 1–30 characters".to_string(),
        ));
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '.' | '#' | '-'))
    {
        return Err(AppError::Validation(
            "tags use letters, numbers and + . # -".to_string(),
        ));
    }
    if !t.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::Validation(
            "a tag starts with a letter or number".to_string(),
        ));
    }
    Ok(canonical_tag(&t))
}

pub fn normalise_slug(raw: &str) -> Result<String, AppError> {
    let s = raw.trim().to_lowercase().replace(' ', "-");
    if s.len() < 2 || s.len() > 30 {
        return Err(AppError::Validation(
            "a vertical slug is 2–30 characters".to_string(),
        ));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::Validation(
            "vertical slugs use letters, numbers and dashes".to_string(),
        ));
    }
    Ok(s)
}

/// Links are user content that other people click. Only http(s), and never a
/// URL carrying credentials: a member who pastes `https://user:pass@host`
/// would publish the password on their profile.
pub fn validate_link_url(raw: &str) -> Result<String, AppError> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 2000 {
        return Err(AppError::Validation("a link needs a URL".to_string()));
    }
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    let url = Url::parse(&with_scheme)
        .map_err(|_| AppError::Validation(format!("`{raw}` is not a URL")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::Validation(
            "links must be http or https".to_string(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::Validation(
            "a link cannot carry a username or password".to_string(),
        ));
    }
    if url.host_str().unwrap_or("").is_empty() {
        return Err(AppError::Validation("that URL has no host".to_string()));
    }
    Ok(url.to_string())
}

/// Short display form of a URL: host + path, no scheme, no trailing slash.
pub fn pretty_url(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("").trim_start_matches("www.");
            let path = u.path().trim_end_matches('/');
            if path.is_empty() || path == "/" {
                host.to_string()
            } else {
                format!("{host}{path}")
            }
        }
        Err(_) => raw.to_string(),
    }
}

pub fn initials(name: &str) -> String {
    let mut out = String::new();
    for word in name.split_whitespace().take(2) {
        if let Some(c) = word.chars().next() {
            out.extend(c.to_uppercase());
        }
    }
    if out.is_empty() {
        "?".to_string()
    } else {
        out
    }
}

pub fn first_line(bio: &str, max: usize) -> String {
    let line = bio.lines().next().unwrap_or("").trim();
    truncate(line, max)
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// A stable, human-looking accent colour derived from the handle, so a member
/// who never picks one still gets a distinct identity instead of grey.
pub fn accent_for(handle: &str) -> String {
    const PALETTE: [&str; 10] = [
        "#7c5cff", "#3ec7a0", "#f2a65a", "#5aa9f2", "#f25aa9", "#c8f25a", "#f2735a", "#5af2e0",
        "#a95af2", "#f2d75a",
    ];
    let mut h: u64 = 0xcbf29ce484222325;
    for b in handle.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    PALETTE[(h % PALETTE.len() as u64) as usize].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_normalised_not_repaired() {
        assert_eq!(normalise_handle("  @Critical.Nexus ").unwrap(), "critical.nexus");
        assert!(normalise_handle("bad handle").is_err());
        assert!(normalise_handle("-leading").is_err());
        assert!(normalise_handle("a").is_err());
    }

    #[test]
    fn tag_spellings_collapse_to_one_lens() {
        // "game dev", "gamedev" and "game-dev" are one tag, or the tuner builds three lenses.
        for raw in ["game dev", "gamedev", "Game-Dev", "#game_dev"] {
            let t = normalise_tag(raw).unwrap();
            assert_eq!(t, "game-dev", "got {t} from {raw}");
        }
        assert_eq!(normalise_tag("  pixelart ").unwrap(), "pixel-art");
        assert_eq!(normalise_tag("a--b---c").unwrap(), "a-b-c");
        assert_eq!(normalise_tag("-leading-").unwrap(), "leading");
        // A tag the table does not know is left alone rather than guessed at.
        assert_eq!(normalise_tag("chiptune").unwrap(), "chiptune");
    }

    #[test]
    fn tags_fold_spaces_and_hashes() {
        assert_eq!(normalise_tag("#Rust Lang").unwrap(), "rust-lang");
        assert_eq!(normalise_tag("c++").unwrap(), "c++");
        // Leading hyphens are a typo, not a different tag.
        assert_eq!(normalise_tag("--x").unwrap(), "x");
        // Symbols that cannot be in a tag are still refused.
        assert!(normalise_tag("!!!").is_err());
        assert!(normalise_tag("").is_err());
    }

    #[test]
    fn a_link_may_not_carry_credentials() {
        assert!(validate_link_url("https://user:pw@example.com/").is_err());
        assert_eq!(
            validate_link_url("example.com").unwrap(),
            "https://example.com/"
        );
        assert!(validate_link_url("javascript:alert(1)").is_err());
        assert!(validate_link_url("ftp://example.com").is_err());
    }

    #[test]
    fn pretty_url_strips_the_noise() {
        assert_eq!(
            pretty_url("https://www.example.com/path/?x=1"),
            "example.com/path"
        );
        assert_eq!(pretty_url("https://example.com/"), "example.com");
    }

    #[test]
    fn accents_are_stable() {
        assert_eq!(accent_for("alice"), accent_for("alice"));
        assert_ne!(accent_for("alice"), accent_for("bob"));
    }
}
