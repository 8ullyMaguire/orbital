use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    /// Absolute origin of the hub, no trailing slash. There is one origin: the adult vertical
    /// used to answer on a second hostname, and DECISIONS D2 retired it.
    pub public_base: String,
    pub session_cookie: String,
    pub visitor_cookie: String,
    /// Secret mixed into the visitor id before hashing, so the stored
    /// session_hash cannot be inverted to a cookie value.
    pub salt: String,
    pub secure_cookies: bool,
    pub bsky_pds: String,
    pub hub_handle: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL is required (sqlx also needs it at compile time)")?,
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8090".into()),
            public_base: std::env::var("PUBLIC_BASE")
                .unwrap_or_else(|_| "http://127.0.0.1:8090".into())
                .trim_end_matches('/')
                .to_string(),
            session_cookie: std::env::var("SESSION_COOKIE").unwrap_or_else(|_| "orbital_s".into()),
            visitor_cookie: std::env::var("VISITOR_COOKIE").unwrap_or_else(|_| "orbital_v".into()),
            salt: std::env::var("SESSION_SALT")
                .unwrap_or_else(|_| "orbital-dev-salt-change-me".into()),
            secure_cookies: std::env::var("SECURE_COOKIES")
                .map(|v| v != "0" && v != "false")
                .unwrap_or(false),
            bsky_pds: std::env::var("BSKY_PDS").unwrap_or_else(|_| "https://bsky.social".into()),
            hub_handle: std::env::var("HUB_HANDLE").unwrap_or_else(|_| "criticalnexus".into()),
        })
    }

    /// Literal config for tests: never `from_env`, because a stray exported
    /// variable in the shell would change what the test measures.
    pub fn for_test(base: &str) -> Self {
        Self {
            database_url: String::new(),
            bind_addr: "127.0.0.1:0".into(),
            public_base: base.trim_end_matches('/').to_string(),
                session_cookie: "orbital_s".into(),
            visitor_cookie: "orbital_v".into(),
            salt: "test-salt".into(),
            secure_cookies: false,
            bsky_pds: "https://bsky.social".into(),
            hub_handle: "hubowner".into(),
        }
    }

}
