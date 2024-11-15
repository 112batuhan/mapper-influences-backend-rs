use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

use super::DatabaseClient;

#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Clone, Debug)]
pub struct GraphUser {
    id: u32,
    avatar_url: String,
    mentions: u32,
    username: String,
}

#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Clone, Debug)]
pub struct GraphInfluence {
    source: u32,
    target: u32,
}

impl DatabaseClient {
    pub async fn get_users_for_graph(&self) -> Result<Vec<GraphUser>, AppError> {
        let graph_users: Vec<GraphUser> = self
            .db
            .query(
                "
                SELECT 
                    meta::id(id) AS id, 
                    count(<-influenced_by) AS mentions, 
                    avatar_url,
                    username
                FROM user
                ",
            )
            .await?
            .take(0)?;
        Ok(graph_users)
    }

    pub async fn get_influences_for_graph(&self) -> Result<Vec<GraphInfluence>, AppError> {
        let graph_influences: Vec<GraphInfluence> = self
            .db
            .query("SELECT meta::id(in) AS source, meta::id(out) AS target FROM influenced_by;")
            .await?
            .take(0)?;
        Ok(graph_influences)
    }
}
