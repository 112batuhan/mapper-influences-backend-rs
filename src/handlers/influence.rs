use axum::{
    extract::{Path, State},
    Extension, Json,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    database::influence::{InfluenceDb, MentionsDb},
    error::AppError,
    jwt::AuthData,
    AppState,
};

#[derive(Deserialize, JsonSchema)]
pub struct Description {
    description: String,
}

// TODO: add a return type for this
pub async fn add_influence(
    Path(influenced_to): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<(), AppError> {
    let target_user = state
        .request
        .get_user_osu(&auth_data.osu_token, influenced_to)
        .await?;
    state.db.upsert_user(target_user, false).await?;
    state
        .db
        .add_influence_relation(auth_data.user_id, influenced_to)
        .await?;
    Ok(())
}

pub async fn delete_influence(
    Path(influenced_to): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<(), AppError> {
    state
        .db
        .remove_influence_relation(auth_data.user_id, influenced_to)
        .await?;
    Ok(())
}

pub async fn add_influence_beatmap(
    Path(beatmap_id): Path<u32>,
    Path(influenced_to): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<(), AppError> {
    let beatmap = state
        .osu_beatmap_multi_requester
        .get_multiple_osu(&[beatmap_id], &auth_data.osu_token)
        .await?;

    if beatmap.is_empty() {
        return Err(AppError::NonExistingMap(beatmap_id));
    }

    state
        .db
        .add_beatmap_to_influence(auth_data.user_id, influenced_to, beatmap_id)
        .await?;
    Ok(())
}

pub async fn remove_influence_beatmap(
    Path(beatmap_id): Path<u32>,
    Path(influenced_to): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<(), AppError> {
    state
        .db
        .remove_beatmap_from_influence(auth_data.user_id, influenced_to, beatmap_id)
        .await?;
    Ok(())
}

pub async fn update_influence_description(
    Path(influenced_to): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(description): Json<Description>,
) -> Result<(), AppError> {
    state
        .db
        .update_influence_description(auth_data.user_id, influenced_to, description.description)
        .await?;
    Ok(())
}

pub async fn update_influence_type(
    Path(influenced_to): Path<u32>,
    Path(type_id): Path<u8>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<(), AppError> {
    state
        .db
        .update_influence_type(auth_data.user_id, influenced_to, type_id)
        .await?;
    Ok(())
}

pub async fn get_user_mentions(
    Path(user_id): Path<u32>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<MentionsDb>>, AppError> {
    let mentions = state.db.get_mentions(user_id).await?;
    Ok(Json(mentions))
}

pub async fn get_user_influences(
    Path(user_id): Path<u32>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InfluenceDb>>, AppError> {
    let influences = state.db.get_influences(user_id).await?;
    Ok(Json(influences))
}
