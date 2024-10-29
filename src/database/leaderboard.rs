use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::AppError, osu_api::Group};

use super::DatabaseClient;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq, Eq)]
pub struct LeaderboardUser {
    id: u32,
    username: String,
    avatar_url: String,
    country_code: String,
    country_name: String,
    mention_count: u32,
    leaderboard_count: u32,
    groups: Vec<Group>,
    ranked_maps: u32,
}

impl DatabaseClient {
    pub async fn leaderboard(
        &self,
        country: Option<String>,
        ranked: bool,
        limit: u32,
        start: u32,
    ) -> Result<Vec<LeaderboardUser>, AppError> {
        let leaderboard: Vec<LeaderboardUser> = self
            .db
            .query(
                "
                SELECT 
                    leaderboard_count, 
                    meta::id(out.id) AS id, 
                    out.username AS username, 
                    out.avatar_url AS avatar_url, 
                    out.country_code AS country_code,
                    out.country_name as country_name,
                    out.groups as groups,
                    out.ranked_and_approved_beatmapset_count 
                        + out.guest_beatmapset_count as ranked_maps,
                    count(out<-influenced_by) as mention_count
                FROM 
                    (SELECT 
                        count() AS leaderboard_count, 
                        out 
                    FROM influenced_by 
                    WHERE $ranked_only = false OR in.ranked_mapper = true 
                    GROUP BY out 
                    ORDER BY leaderboard_count DESC
                    )
                WHERE $country = none or out.country_code = $country
                LIMIT $limit
                START $start;
                ",
            )
            .bind(("country", country))
            .bind(("ranked_only", ranked))
            .bind(("limit", limit))
            .bind(("start", start))
            .await?
            .take(0)?;
        Ok(leaderboard)
    }
}
