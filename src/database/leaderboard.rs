use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    osu_api::{BeatmapEnum, OsuBeatmapSmall},
};

use super::{user::UserSmall, DatabaseClient};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq, Eq)]
/// `LeaderboardUser` type
pub struct LeaderboardUser {
    user: UserSmall,
    /// leaderboard mention count
    count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq)]
/// `LeaderboardBeatmap` type
pub struct LeaderboardBeatmap {
    #[schemars(with = "OsuBeatmapSmall")]
    pub beatmap: BeatmapEnum,
    /// Amount of times that this map has been added to the mentions
    pub count: u32,
}

impl DatabaseClient {
    pub async fn user_leaderboard(
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
                    count, 
                    meta::id(out.id) AS user.id, 
                    out.username AS user.username, 
                    out.avatar_url AS user.avatar_url, 
                    out.country_code AS user.country_code,
                    out.country_name as user.country_name,
                    out.groups as user.groups,
                    out.ranked_and_approved_beatmapset_count 
                        + out.guest_beatmapset_count as user.ranked_maps,
                    count(out<-influenced_by) as user.mentions,
                    out.previous_usernames as user.previous_usernames
                FROM 
                    (SELECT 
                        count() AS count, 
                        out 
                    FROM influenced_by 
                    WHERE $ranked_only = false OR in.ranked_mapper = true 
                    GROUP BY out 
                    ORDER BY count DESC
                    )
                WHERE $country = none or out.country_code = $country
                ORDER count DESC
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

    pub async fn beatmap_leaderboard(
        &self,
        ranked: bool,
        limit: u32,
        start: u32,
    ) -> Result<Vec<LeaderboardBeatmap>, AppError> {
        let leaderboard: Vec<LeaderboardBeatmap> = self
            .db
            .query(
                "
                SELECT *
                FROM (
                    SELECT 
                        beatmap,
                        count(beatmap) as count 
                    FROM (
                        (
                            SELECT beatmaps
                            FROM influenced_by
                            WHERE $ranked_only = false OR <-user.ranked_mapper.at(0) = true
                        )
                        .map(|$val| $val.values())
                        .flatten()
                        .flatten()
                        .map(|$val| {beatmap: $val})
                    )
                    GROUP BY beatmap
                )
                ORDER BY count DESC
                START $start
                LIMIT $limit;
                ",
            )
            .bind(("ranked_only", ranked))
            .bind(("limit", limit))
            .bind(("start", start))
            .await?
            .take(0)?;
        Ok(leaderboard)
    }
}
