use axum::{
    extract::{Path, State},
    Extension, Json,
};
use futures::try_join;
use itertools::Itertools;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    database::influence::Influence,
    error::AppError,
    jwt::AuthData,
    osu_api::{BeatmapEnum, GetID},
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

    try_join!(
        state.db.upsert_user(target_user, false),
        state
            .db
            .add_influence_relation(auth_data.user_id, influenced_to)
    )?;
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
        .cached_combined_requester
        .clone()
        .get_beatmaps_only(&[beatmap_id], &auth_data.osu_token)
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
) -> Result<Json<Vec<Influence>>, AppError> {
    let mentions = state.db.get_mentions(user_id).await?;
    Ok(Json(mentions))
}

pub async fn get_user_influences(
    Path(user_id): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Influence>>, AppError> {
    let mut influences = state.db.get_influences(user_id).await?;

    let beatmaps_to_request: Vec<u32> = influences
        .iter()
        .flat_map(|influence| &influence.beatmaps)
        .map(|maps| maps.get_id())
        .unique()
        .collect();

    let beatmaps = state
        .cached_combined_requester
        .clone()
        .get_beatmaps_with_user(&beatmaps_to_request, &auth_data.osu_token)
        .await?;

    // Influences converted with beatmap data
    influences.iter_mut().for_each(|influence| {
        let new_beatmaps = influence
            .beatmaps
            .iter()
            .filter_map(|beatmap| {
                // it's not ok to use remove here
                // there could be beatmaps used more than once
                let beatmap = beatmaps.get(&beatmap.get_id())?;
                Some(BeatmapEnum::All(beatmap.clone()))
            })
            .collect();
        influence.beatmaps = new_beatmaps;
    });

    Ok(Json(influences))
}
