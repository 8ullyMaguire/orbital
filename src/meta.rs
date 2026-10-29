//! A link is not a title. Fetch the page's own words once and keep them.

use anyhow::Result;
use url::Url;

/// What a page says about itself.
#[derive(Debug, Clone, Default)]
pub struct Meta {
    pub title: Option<String>,
    pub favicon: Option<String>,
}

/// Fetch a page and read `<title>` and `<link rel="icon">` out of its head.
///
/// Deliberately not a parser dependency: we only need the first kilobyte of the head, the tags we
/// want are required to be well-formed, and a full HTML parser for two fields is a liability.
/// Anything unparseable is simply "no metadata" - never an error the caller must handle.
pub async fn page_metadata(client: &reqwest::Client, raw: &str) -> Result<Meta> {
    let url = Url::parse(raw)?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("not http");
    }
    let host = url.host_str().ok_or_else(|| anyhow::anyhow!("no host"))?;
    // Same guard as the link checker: a member's URL must not make the server read its own network.
    if !crate::health::host_is_public(host).await.unwrap_or(false) {
        anyhow::bail!("not a public host");
    }

    let resp = client
        .get(url.clone())
        .header(reqwest::header::RANGE, "bytes=0-65535")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await?;
    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        anyhow::bail!("status {}", resp.status());
    }
    let body = resp.text().await.unwrap_or_default();
    Ok(parse(&body, &url))
}

/// The parsing half, so it can be tested without a network.
pub fn parse(body: &str, base: &Url) -> Meta {
    let title = extract_title(body);
    let favicon = extract_icon(body, base).or_else(|| {
        // No declaration: /favicon.ico is a convention worth trying, and a 404 costs one request
        // when it is finally rendered by a browser that would have asked anyway.
        base.join("/favicon.ico").ok().map(|u| u.to_string())
    });
    Meta { title, favicon }
}

fn extract_title(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let end = lower[open_end..].find("</title>")? + open_end;
    let raw = html_unescape(body[open_end..end].trim());
    let cleaned = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        None
    } else {
        Some(truncate_chars(&cleaned, 120))
    }
}

fn extract_icon(body: &str, base: &Url) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    // Scan every <link ...> in order; the first one that says rel contains "icon".
    let mut from = 0usize;
    while let Some(i) = lower[from..].find("<link") {
        let start = from + i;
        let close = lower[start..].find('>')? + start;
        let tag = &body[start..close];
        let tag_lower = &lower[start..close];
        if tag_lower.contains("icon") {
            if let Some(href) = attr(tag, "href") {
                if let Ok(u) = base.join(&href) {
                    if matches!(u.scheme(), "http" | "https") {
                        return Some(u.to_string());
                    }
                }
            }
        }
        from = close + 1;
        if from >= lower.len() {
            break;
        }
    }
    None
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let hay = tag.to_ascii_lowercase();
    let needle = format!("{name}=");
    let mut from = 0usize;
    while let Some(i) = hay[from..].find(&needle) {
        let at = from + i + needle.len();
        let rest = &tag[at..];
        let (quote, rest) = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => (q, &rest[q.len_utf8()..]),
            _ => (' ', rest),
        };
        let end = if quote == ' ' {
            rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len())
        } else {
            rest.find(quote).unwrap_or(rest.len())
        };
        let value = rest[..end].trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
        from = at;
    }
    None
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/blog/post").unwrap()
    }

    #[test]
    fn reads_the_title_and_collapses_whitespace() {
        let m = parse(
            "<html><head><title>\n  Ana &amp; the   Long\n Night </title></head></html>",
            &base(),
        );
        assert_eq!(m.title.as_deref(), Some("Ana & the Long Night"));
    }

    #[test]
    fn resolves_a_relative_icon_against_the_page() {
        let m = parse(
            r#"<head><link rel="icon" href="/static/fav.png"></head>"#,
            &base(),
        );
        assert_eq!(m.favicon.as_deref(), Some("https://example.com/static/fav.png"));
    }

    #[test]
    fn falls_back_to_favicon_ico() {
        let m = parse("<head><title>x</title></head>", &base());
        assert_eq!(m.favicon.as_deref(), Some("https://example.com/favicon.ico"));
    }

    #[test]
    fn a_page_without_a_head_is_not_an_error() {
        let m = parse("", &base());
        assert!(m.title.is_none());
    }

    #[test]
    fn a_javascript_icon_is_refused() {
        let m = parse(r#"<link rel="icon" href="javascript:alert(1)">"#, &base());
        assert_eq!(
            m.favicon.as_deref(),
            Some("https://example.com/favicon.ico"),
            "the convention, not the script"
        );
    }

    #[test]
    fn long_titles_are_cut() {
        let long = format!("<title>{}</title>", "a".repeat(400));
        let m = parse(&long, &base());
        assert_eq!(m.title.unwrap().chars().count(), 120);
    }
}
