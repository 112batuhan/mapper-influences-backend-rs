use std::sync::Arc;

use axum::{
    debug_handler,
    extract::{Path, Query, State},
    Extension, Json,
};
use cached::proc_macro::cached;

use crate::{
    error::AppError,
    jwt::AuthData,
    osu_api::{BeatmapOsu, OsuSearchUserResponse, UserOsu},
    AppState,
};

#[debug_handler]
#[cached(time = 21600, key = "u32", convert = r#"{user_id}"#, result = true)]
pub async fn osu_user(
    Path(user_id): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<UserOsu>, AppError> {
    let user_osu = state
        .request
        .get_user_osu(&auth_data.osu_token, user_id)
        .await?;
    Ok(Json(user_osu))
}

#[cached(time = 86400, key = "u32", convert = r#"{beatmap_id}"#, result = true)]
pub async fn osu_beatmap(
    Path(beatmap_id): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<BeatmapOsu>, AppError> {
    let beatmap_osu = state
        .request
        .get_beatmap_osu(&auth_data.osu_token, beatmap_id)
        .await?;
    Ok(Json(beatmap_osu))
}

#[cached(
    time = 240,
    key = "String",
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
    time = 240,
    key = "String",
    convert = r#"{query.clone()}"#,
    result = true
)]
pub async fn osu_beatmap_search(
    Query(query): Query<String>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<OsuSearchUserResponse>, AppError> {
    let beatmap_search_osu = state
        .request
        .search_user_osu(&auth_data.osu_token, &query)
        .await?;
    Ok(Json(beatmap_search_osu))
}
