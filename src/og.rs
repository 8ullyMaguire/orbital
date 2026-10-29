//! Social cards: one image per member, rendered as SVG and (when a rasteriser exists) as PNG.
//!
//! The card is generated, not uploaded: a directory of people should not ask each of them to bring
//! a 1200×630 photograph before their link is worth sharing.

/// The card, as SVG text. 1200×630 is the size every network crops to.
pub fn card(name: &str, handle: &str, links: i64, base: &str) -> String {
    let name = escape(name);
    let handle = escape(handle);
    let host = escape(base.trim_start_matches("https://").trim_start_matches("http://"));
    let plural = if links == 1 { "link" } else { "links" };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630" role="img" aria-label="@{handle} on {host}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0b0f17"/>
      <stop offset="1" stop-color="#141c2b"/>
    </linearGradient>
  </defs>
  <rect width="1200" height="630" fill="url(#bg)"/>
  <circle cx="1080" cy="90" r="260" fill="#2b6cb0" opacity="0.18"/>
  <circle cx="140" cy="560" r="200" fill="#805ad5" opacity="0.14"/>
  <text x="80" y="150" font-family="Inter, system-ui, sans-serif" font-size="34" fill="#7f8ea3">{host}</text>
  <text x="80" y="330" font-family="Inter, system-ui, sans-serif" font-size="86" font-weight="700" fill="#f4f6fa">{name}</text>
  <text x="80" y="410" font-family="Inter, system-ui, sans-serif" font-size="46" fill="#9fb0c7">@{handle}</text>
  <text x="80" y="520" font-family="Inter, system-ui, sans-serif" font-size="38" fill="#6f7f96">{links} {plural}</text>
  <rect x="80" y="560" width="120" height="6" rx="3" fill="#2b6cb0"/>
</svg>
"##
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_names_the_member_and_the_host() {
        let svg = card("Ana", "ana", 7, "https://orbital.polarisocial.xyz");
        assert!(svg.contains(">Ana<"));
        assert!(svg.contains("@ana"));
        assert!(svg.contains("7 links"));
        assert!(svg.contains("orbital.polarisocial.xyz"));
    }

    #[test]
    fn a_hostile_display_name_cannot_inject_markup() {
        let svg = card("<script>alert(1)</script>", "ana", 1, "https://x.test");
        assert!(!svg.contains("<script>"));
        assert!(svg.contains("&lt;script&gt;"));
    }

    #[test]
    fn one_link_is_not_one_links() {
        assert!(card("A", "a", 1, "https://x.test").contains("1 link<"));
    }
}
