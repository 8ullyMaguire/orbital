use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct User {
    pub id: Uuid,
    pub handle: String,
    pub display_name: String,
    pub bio: String,
    pub accent: String,
    pub avatar_url: Option<String>,
    pub bsky_handle: Option<String>,
    pub bsky_verified_at: Option<DateTime<Utc>>,
    pub verify_token: String,
    pub trust_level: i16,
    pub nsfw_enabled: bool,
    pub is_owner: bool,
    pub hidden: bool,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct LinkRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub vertical_slug: Option<String>,
    pub url: String,
    pub label: String,
    pub icon: String,
    pub position: i32,
    pub click_count: i64,
    pub hidden: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct VerticalRow {
    pub slug: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub nsfw: bool,
    pub sort: i32,
    pub min_members: i32,
}

/// One member, measured **in one vertical's context**. `vertical_slug` is the
/// context the stats were computed for; it is empty for queries that do not
/// have one.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemberStat {
    pub id: Uuid,
    pub handle: String,
    pub display_name: String,
    pub bio: String,
    pub accent: String,
    pub avatar_url: Option<String>,
    pub trust_level: i16,
    pub bsky_handle: Option<String>,
    pub nsfw_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub role: String,
    pub visitors: i64,
    pub visitors_7d: i64,
    pub clicks: i64,
    pub tags: Vec<String>,
    pub verticals: Vec<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VerticalCount {
    pub slug: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub nsfw: bool,
    pub sort: i32,
    pub min_members: i32,
    pub members: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct DayPoint {
    pub day: NaiveDate,
    pub views: i64,
    pub clicks: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct LinkStat {
    pub id: Uuid,
    pub label: String,
    pub url: String,
    pub clicks: i64,
    pub visitors: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Bucket {
    pub key: String,
    pub clicks: i64,
    pub visitors: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct FlagRow {
    pub id: Uuid,
    pub target_user_id: Uuid,
    pub target_handle: String,
    pub vertical_slug: Option<String>,
    pub reason: String,
    pub detail: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterInput {
    pub handle: String,
    pub password: String,
    #[serde(default)]
    pub display_name: String,
    pub vertical: String,
    #[serde(default)]
    pub bio: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub handle: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ProfilePatch {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub accent: Option<String>,
    pub avatar_url: Option<String>,
    pub primary_vertical: Option<String>,
    pub secondary_verticals: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub nsfw_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct LinkInput {
    pub url: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub vertical_slug: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LinkPatch {
    pub url: Option<String>,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub vertical_slug: Option<Option<String>>,
    pub hidden: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderInput {
    pub order: Vec<Uuid>,
}
