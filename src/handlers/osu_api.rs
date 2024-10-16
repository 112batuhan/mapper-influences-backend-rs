use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use cached::proc_macro::cached;

use crate::{
    custom_cache::CustomCache,
    error::AppError,
    jwt::AuthData,
    osu_api::{OsuSearchMapResponse, OsuSearchUserResponse},
    AppState,
};

#[cached(
    ty = "CustomCache<String, Json<OsuSearchUserResponse>>",
    create = "{CustomCache::new(240)}",
    convert = r#"{query.clone()}"#,
    result = true
)]

pub async fn osu_user_search(
    Path(query): Path<String>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<OsuSearchUserResponse>, AppError> {
    let user_search_osu = state
        .request
        .search_user_osu(&auth_data.osu_token, &query)
        .await?;
    Ok(Json(user_search_osu))
}

#[cached(
    ty = "CustomCache<String, Json<OsuSearchMapResponse>>",
    create = "{CustomCache::new(240)}",
    convert = r#"{query.clone()}"#,
    result = true
)]
pub async fn osu_beatmap_search(
    Query(query): Query<String>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<OsuSearchMapResponse>, AppError> {
    let beatmap_search_osu = state
        .request
        .search_map_osu(&auth_data.osu_token, &query)
        .await?;
    Ok(Json(beatmap_search_osu))
}
