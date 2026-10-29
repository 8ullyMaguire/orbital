//! Two languages, one directory.
//!
//! The site is announced in Spanish and English, so it should read in both. The mechanism is the
//! smallest one that works: a cookie (set by `/lang/es`), an `Accept-Language` fallback for the
//! first visit, and a static dictionary keyed by string. Templates ask for a key; a key that is not
//! in the table falls back to English, so a missing translation is a bug you can see, not a crash.
//!
//! Every key used in a template is checked by a test in this module: it greps `templates/` for
//! `layout.t("…")` and fails if the key is not here. That is the whole reason a static table is
//! safe to hand-edit.

use axum::http::HeaderMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Es,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        match code.trim().to_ascii_lowercase().as_str() {
            "en" | "en-gb" | "en-us" => Some(Lang::En),
            "es" | "es-es" | "es-419" | "es-mx" => Some(Lang::Es),
            _ => None,
        }
    }

    /// The cookie wins; then the browser's own preference; then English.
    ///
    /// This is why a first-time visitor from a Spanish browser sees Spanish without clicking
    /// anything, and why the choice sticks afterwards.
    pub fn from_headers(headers: &HeaderMap) -> Lang {
        if let Some(cookie) = crate::auth::cookie_value(headers, "orbital_lang") {
            if let Some(lang) = Lang::from_code(&cookie) {
                return lang;
            }
        }
        if let Some(header) = headers
            .get(axum::http::header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
        {
            // "es-ES,es;q=0.9,en;q=0.8" - the first tag decides, which is what the visitor asked
            // for. Reading the whole q-list would be more correct and less predictable.
            if let Some(first) = header.split(',').next() {
                if let Some(lang) = Lang::from_code(first) {
                    return lang;
                }
            }
        }
        Lang::En
    }
}

/// One string, in every language we publish.
struct Entry(&'static str, &'static str, &'static str);

/// The dictionary. `(key, english, spanish)`.
///
/// Kept flat and alphabetical by section: a nested structure would need a lookup path in every
/// template, and the keys are already namespaced with dots.
static STRINGS: &[Entry] = &[
    // ---- chrome (base.html), which is on every page
    Entry("skip", "Skip to content", "Ir al contenido"),
    Entry("nav.main", "Main", "Principal"),
    Entry("nav.directory", "Directory", "Directorio"),
    Entry("nav.verticals", "Verticals", "Verticales"),
    Entry("nav.search", "Search", "Buscar"),
    Entry("cta.add", "Add your profile", "Añade tu perfil"),
    Entry("foot.browse", "Browse", "Explorar"),
    Entry("foot.everything", "Everything", "Todo"),
    Entry("foot.search", "Search people", "Buscar personas"),
    Entry("foot.feed_json", "JSON feed", "Feed JSON"),
    Entry("foot.rss", "RSS", "RSS"),
    Entry("foot.join", "Join", "Únete"),
    Entry("foot.create", "Create a profile", "Crear un perfil"),
    Entry("foot.signin", "Sign in", "Entrar"),
    Entry("foot.how", "How it works", "Cómo funciona"),
    Entry("foot.note", "a directory, not an algorithm. No ads, no trackers, no third-party scripts.",
          "un directorio, no un algoritmo. Sin anuncios, sin rastreadores, sin scripts de terceros."),
    Entry("lang.switch", "Español", "English"),
    // ---- home
    Entry("home.trending", "Trending this week", "Tendencias esta semana"),
    Entry("home.new", "New here", "Recién llegados"),
    Entry("home.picks", "Worth a look", "Merece la pena"),
    Entry("home.members", "people", "personas"),
    Entry("home.clicks", "visits sent", "visitas enviadas"),
];

/// What a template gets for a key that is not in the table. The test below makes this unreachable
/// from a template, so it exists for the one case a test cannot see: a key added at runtime.
pub const MISSING: &str = "(missing translation)";

/// The string for a key, or the English one, or `MISSING`.
pub fn t(lang: Lang, key: &str) -> &'static str {
    match STRINGS.iter().find(|e| e.0 == key) {
        Some(Entry(_, en, es)) => match lang {
            Lang::En => en,
            Lang::Es => es,
        },
        None => {
            tracing::warn!(key, "missing translation");
            MISSING
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cookie_beats_the_browser_and_the_browser_beats_english() {
        let mut h = HeaderMap::new();
        assert_eq!(Lang::from_headers(&h), Lang::En);

        h.insert("accept-language", "es-ES,es;q=0.9".parse().unwrap());
        assert_eq!(Lang::from_headers(&h), Lang::Es);

        h.insert("cookie", "orbital_lang=en".parse().unwrap());
        assert_eq!(
            Lang::from_headers(&h),
            Lang::En,
            "an explicit choice outranks the browser's preference"
        );
    }

    #[test]
    fn every_key_used_in_a_template_exists() {
        // The invariant that makes a hand-edited dictionary safe.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let mut checked = 0usize;
        for entry in std::fs::read_dir(&dir).expect("templates") {
            let path = entry.expect("entry").path();
            let Ok(body) = std::fs::read_to_string(&path) else {
                continue;
            };
            for part in body.split("layout.t(\"").skip(1) {
                let key = part.split('"').next().unwrap_or("");
                if key.is_empty() {
                    continue;
                }
                assert!(
                    STRINGS.iter().any(|e| e.0 == key),
                    "{}: template asks for unknown key {key}",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no template uses layout.t yet - the check is inert");
    }

    #[test]
    fn no_translation_is_left_empty_or_untranslated() {
        for Entry(key, en, es) in STRINGS {
            assert!(!key.is_empty());
            assert!(!en.trim().is_empty(), "{key} has no English");
            assert!(!es.trim().is_empty(), "{key} has no Spanish");
            // A copy-paste that forgot to translate is the most common bug in a table like this.
            if *key != "foot.rss" && *key != "lang.switch" {
                assert_ne!(en, es, "{key} is the same in both languages");
            }
        }
    }

    #[test]
    fn an_unknown_key_says_so_rather_than_going_blank() {
        assert_eq!(t(Lang::Es, "not.a.key"), MISSING);
    }
}
