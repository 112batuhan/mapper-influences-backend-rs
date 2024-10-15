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
    count: u32,
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
                    meta::id(id) AS id, 
                    username,
                    avatar_url,
                    country,
                    count(<-influenced_by) as mention_count,
                    count(<-influenced_by<-(user WHERE ranked_mapper = true OR $ranked_only = false )) AS count                
                FROM user 
                WHERE($country = none or country = $country) 
                ORDER BY count DESC
                LIMIT $limit
                START $start;
                ",
            )
            .bind(("country", country))
            .bind(("ranked_board", ranked))
            .bind(("limit", limit))
            .bind(("start", start))
            .await?
            .take(0)?;
        Ok(leaderboard)
    }
}
