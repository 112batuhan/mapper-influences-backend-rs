use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

use super::DatabaseClient;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Leaderboard {
    id: u32,
    username: String,
    avatar_url: String,
    country: String,
    mention_count: u32,
    leaderboard_count: u32,
}

impl DatabaseClient {
    pub async fn leaderboard(
        &self,
        country: Option<String>,
        ranked: bool,
        limit: u32,
        start: u32,
    ) -> Result<Vec<Leaderboard>, AppError> {
        let leaderboard: Vec<Leaderboard> = self
            .db
            .query(
                "
                SELECT 
                    leaderboard_count, 
                    meta::id(out.id) AS id, 
                    out.username AS username, 
                    out.avatar_url AS avatar_url, 
                    out.country AS country,
                    count(out<-influenced_by) as mention_count
                FROM 
                    (SELECT 
                        count() AS leaderboard_count, 
                        out 
                    FROM influenced_by 
                    WHERE $ranked_only = false OR <-user.ranked_mapper.at(0) = true 
                    GROUP BY out 
                    ORDER BY leaderboard_count DESC
                    )
                WHERE $country = none or out.country = $country
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
