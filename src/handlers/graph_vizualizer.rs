use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{cache::RedisCache, database::graph_vizualizer::GraphData, error::AppError, AppState};

const GRAPH_KEY: &str = "graph:data";

pub struct GraphCache {
    cache: Arc<RedisCache>,
    expire_in: u64,
}

impl GraphCache {
    pub fn new(cache: Arc<RedisCache>, expire_in: u64) -> Self {
        GraphCache { cache, expire_in }
    }

    pub async fn update(&self, data: &GraphData) -> Result<(), AppError> {
        self.cache.set(GRAPH_KEY, data, self.expire_in).await
    }

    pub async fn get_data(&self) -> Result<Option<GraphData>, AppError> {
        self.cache.get(GRAPH_KEY).await
    }
}

pub async fn get_graph_data(
    State(state): State<Arc<AppState>>,
) -> Result<Json<GraphData>, AppError> {
    if let Some(cached_graph) = state.graph_cache.get_data().await? {
        return Ok(Json(cached_graph));
    }

    let graph_data = state.db.get_graph_data().await?;
    state.graph_cache.update(&graph_data).await?;

    Ok(Json(graph_data))
}
