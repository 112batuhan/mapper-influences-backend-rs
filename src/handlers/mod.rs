use schemars::JsonSchema;
use serde::Deserialize;

pub mod activity;
pub mod auth;
pub mod influence;
pub mod leaderboard;
pub mod osu_api;
pub mod user;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PaginationQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    start: u32,
}
fn default_limit() -> u32 {
    100
}
