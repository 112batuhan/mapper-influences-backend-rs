use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use axum::{extract::State, Json};

use crate::{database::graph_vizualizer::GraphData, error::AppError, AppState};

pub struct GraphCacheInner {
    pub data: Option<GraphData>,
    pub last_instant: Option<Instant>,
    pub expire_in: Duration,
}

pub struct GraphCache(RwLock<GraphCacheInner>);

impl GraphCache {
    pub fn new(expire_in: u64) -> Self {
        GraphCache(RwLock::new(GraphCacheInner {
            data: None,
            last_instant: None,
            expire_in: Duration::from_secs(expire_in),
        }))
    }

    pub fn update(&self, data: GraphData) -> Result<(), AppError> {
        let mut locked = self.0.write().map_err(|_| AppError::RwLock)?;
        locked.data = Some(data);
        locked.last_instant = Some(Instant::now());
        Ok(())
    }

    pub fn get_data(&self) -> Option<GraphData> {
        let locked = self.0.read().ok()?;
        if let (Some(data), Some(last_instant)) = (locked.data.clone(), locked.last_instant) {
            if last_instant.elapsed() > locked.expire_in {
                None
            } else {
                Some(data)
            }
        } else {
            None
        }
    }
}

pub async fn get_graph_data(
    State(state): State<Arc<AppState>>,
) -> Result<Json<GraphData>, AppError> {
    if let Some(cached_graph) = state.graph_cache.get_data() {
        return Ok(Json(cached_graph));
    }

    let graph_data = state.db.get_graph_data().await?;
    state.graph_cache.update(graph_data.clone())?;

    Ok(Json(graph_data))
}
