use axum::{
    extract::{Path, Query, State},
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

use super::{swap_beatmaps, PaginationQuery};

#[derive(Deserialize, JsonSchema)]
pub struct Description {
    description: String,
}

pub async fn add_influence(
    Path(influenced_to): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Influence>, AppError> {
    let target_user = state
        .request
        .get_user_osu(&auth_data.osu_token, influenced_to)
        .await?;

    // We don't need to swap beatmaps here
    // There should be no maps in fresh influence relations
    let (_, influence) = try_join!(
        state.db.upsert_user(target_user, false),
        state
            .db
            .add_influence_relation(auth_data.user_id, influenced_to)
    )?;
    Ok(Json(influence))
}

pub async fn delete_influence(
    Path(influenced_to): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Influence>, AppError> {
    let mut influence = state
        .db
        .remove_influence_relation(auth_data.user_id, influenced_to)
        .await?;
    if let Some(beatmaps) = influence.beatmaps.as_mut() {
        swap_beatmaps(
            state.cached_combined_requester.clone(),
            &auth_data.osu_token,
            beatmaps,
        )
        .await?;
    }
    Ok(Json(influence))
}

pub async fn add_influence_beatmap(
    Path((influenced_to, beatmap_id)): Path<(u32, u32)>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Influence>, AppError> {
    let beatmap = state
        .cached_combined_requester
        .clone()
        .get_beatmaps_only(&[beatmap_id], &auth_data.osu_token)
        .await?;

    if beatmap.is_empty() {
        return Err(AppError::NonExistingMap(beatmap_id));
    }

    let mut influence = state
        .db
        .add_beatmap_to_influence(auth_data.user_id, influenced_to, beatmap_id)
        .await?;

    if let Some(beatmaps) = influence.beatmaps.as_mut() {
        swap_beatmaps(
            state.cached_combined_requester.clone(),
            &auth_data.osu_token,
            beatmaps,
        )
        .await?;
    }

    Ok(Json(influence))
}

pub async fn remove_influence_beatmap(
    Path((influenced_to, beatmap_id)): Path<(u32, u32)>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Influence>, AppError> {
    let mut influence = state
        .db
        .remove_beatmap_from_influence(auth_data.user_id, influenced_to, beatmap_id)
        .await?;

    if let Some(beatmaps) = influence.beatmaps.as_mut() {
        swap_beatmaps(
            state.cached_combined_requester.clone(),
            &auth_data.osu_token,
            beatmaps,
        )
        .await?;
    }
    Ok(Json(influence))
}

pub async fn update_influence_description(
    Path(influenced_to): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(description): Json<Description>,
) -> Result<Json<Influence>, AppError> {
    const MAX_DESC_LENGTH: usize = 5000;
    if description.description.len() > MAX_DESC_LENGTH {
        return Err(AppError::BioTooLong);
    }
    let mut influence = state
        .db
        .update_influence_description(auth_data.user_id, influenced_to, description.description)
        .await?;

    if let Some(beatmaps) = influence.beatmaps.as_mut() {
        swap_beatmaps(
            state.cached_combined_requester.clone(),
            &auth_data.osu_token,
            beatmaps,
        )
        .await?;
    }
    Ok(Json(influence))
}

pub async fn update_influence_type(
    Path((influenced_to, type_id)): Path<(u32, u8)>,

    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Influence>, AppError> {
    let mut influence = state
        .db
        .update_influence_type(auth_data.user_id, influenced_to, type_id)
        .await?;

    if let Some(beatmaps) = influence.beatmaps.as_mut() {
        swap_beatmaps(
            state.cached_combined_requester.clone(),
            &auth_data.osu_token,
            beatmaps,
        )
        .await?;
    }
    Ok(Json(influence))
}

pub async fn get_user_mentions(
    Query(pagination): Query<PaginationQuery>,
    Path(user_id): Path<u32>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Influence>>, AppError> {
    let mentions = state
        .db
        .get_mentions(user_id, pagination.start, pagination.limit)
        .await?;
    Ok(Json(mentions))
}

pub async fn get_user_influences(
    Query(pagination): Query<PaginationQuery>,
    Path(user_id): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Influence>>, AppError> {
    let mut influences = state
        .db
        .get_influences(user_id, pagination.start, pagination.limit)
        .await?;

    let beatmaps_to_request: Vec<u32> = influences
        .iter()
        .flat_map(|influence| &influence.beatmaps)
        .flatten()
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
            .flatten()
            .filter_map(|beatmap| {
                // it's not ok to use remove here
                // there could be beatmaps used more than once
                let beatmap = beatmaps.get(&beatmap.get_id())?;
                Some(BeatmapEnum::All(beatmap.clone()))
            })
            .collect();
        influence.beatmaps = Some(new_beatmaps);
    });

    Ok(Json(influences))
}
