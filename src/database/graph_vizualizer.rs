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
    influenced_by: u32,
}

#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Clone, Debug)]
pub struct GraphInfluence {
    source: u32,
    target: u32,
    influence_type: u8,
}

#[derive(Serialize, JsonSchema, Clone)]
pub struct GraphData {
    pub nodes: Vec<GraphUser>,
    pub links: Vec<GraphInfluence>,
}

impl DatabaseClient {
    /// These two select queries are combined into one. The goal is to keep the data consistent
    /// with each other to avoid errors in graphs. It's an edge case but can happen if load is
    /// high. And since we cache the results, the error will stay on UI for the duration of the
    /// cache. Not optimal. If it happens regardless, then use transactions.
    pub async fn get_graph_data(&self) -> Result<GraphData, AppError> {
        let mut query_result = self
            .db
            .query(
                "
                SELECT 
                    meta::id(id) AS id, 
                    count(<-influenced_by) AS mentions,
                    count(->influenced_by) AS influenced_by,
                    avatar_url,
                    username
                FROM user
                WHERE 
                    count(<-influenced_by) > 0 
                    OR count(->influenced_by) > 0;

                SELECT meta::id(in) AS source, meta::id(out) AS target, influence_type FROM influenced_by;
                ",
            )
            .await?;
        Ok(GraphData {
            nodes: query_result.take(0)?,
            links: query_result.take(1)?,
        })
    }
}
