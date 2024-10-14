use serde::{Deserialize, Serialize};

use crate::error::AppError;

use super::{CustomId, DatabaseClient};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Leaderboard {
    id: CustomId,
    country: String,
    count: u32,
    influenced: Vec<CustomId>,
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
                "SELECT 
                    id,
                    country,
                    <-influenced_by<-(user WHERE (ranked = true OR $ranked_board = false) 
                        AND ($country = none or country = $country)) AS influenced, 
                    count(<-influenced_by<-(user WHERE (ranked = true OR $ranked_board = false) 
                        AND ($country = none or country = $country))) AS count
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
