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

use super::{
    check_multiple_maps, swap_beatmaps, BeatmapRequest, PaginationQuery, PathInfluencedTo,
    PathUserBeatmapIds, PathUserId, PathUserTypeId,
};

#[derive(Deserialize, JsonSchema)]
pub struct Description {
    description: String,
}

/// `InfluenceCreationOptions` type. Optional fields to override defaults
#[derive(Deserialize, JsonSchema)]
pub struct InfluenceCreationOptions {
    pub influence_type: Option<u8>,
    pub description: Option<String>,
    pub beatmaps: Option<Vec<u32>>,
    #[serde(alias = "userId")]
    pub user_id: String,
}

pub async fn add_influence(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(options): Json<InfluenceCreationOptions>,
) -> Result<Json<Influence>, AppError> {
    let influenced_to = options.user_id.parse::<u32>()?;

    let target_user = state
        .request
        .get_user_osu(&auth_data.osu_token, influenced_to)
        .await?;

    if let Some(influence_beatmaps) = &options.beatmaps {
        check_multiple_maps(
            state.cached_combined_requester.clone(),
            &auth_data.osu_token,
            influence_beatmaps,
        )
        .await?;
    }

    let (_, mut influence) = try_join!(
        state.db.upsert_user(target_user),
        state
            .db
            .add_influence_relation(auth_data.user_id, influenced_to, options)
    )?;

    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut influence.beatmaps,
    )
    .await?;

    Ok(Json(influence))
}

pub async fn delete_influence(
    Path(influenced_to): Path<PathInfluencedTo>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Influence>, AppError> {
    let mut influence = state
        .db
        .remove_influence_relation(auth_data.user_id, influenced_to.value)
        .await?;
    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut influence.beatmaps,
    )
    .await?;

    Ok(Json(influence))
}

pub async fn add_influence_beatmap(
    Path(path): Path<PathInfluencedTo>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(beatmaps): Json<BeatmapRequest>,
) -> Result<Json<Influence>, AppError> {
    let beatmaps: Vec<u32> = beatmaps.ids.into_iter().collect();
    check_multiple_maps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &beatmaps,
    )
    .await?;

    let mut influence = state
        .db
        .add_beatmap_to_influence(auth_data.user_id, path.value, beatmaps)
        .await?;

    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut influence.beatmaps,
    )
    .await?;

    Ok(Json(influence))
}

pub async fn remove_influence_beatmap(
    Path(path): Path<PathUserBeatmapIds>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Influence>, AppError> {
    let mut influence = state
        .db
        .remove_beatmap_from_influence(auth_data.user_id, path.influenced_to, path.beatmap_id)
        .await?;

    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut influence.beatmaps,
    )
    .await?;

    Ok(Json(influence))
}

pub async fn update_influence_description(
    Path(influenced_to): Path<PathInfluencedTo>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(description): Json<Description>,
) -> Result<Json<Influence>, AppError> {
    const MAX_DESC_LENGTH: usize = 5000;
    if description.description.len() > MAX_DESC_LENGTH {
        return Err(AppError::StringTooLong);
    }
    let mut influence = state
        .db
        .update_influence_description(
            auth_data.user_id,
            influenced_to.value,
            description.description,
        )
        .await?;

    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut influence.beatmaps,
    )
    .await?;
    Ok(Json(influence))
}

pub async fn update_influence_type(
    Path(path): Path<PathUserTypeId>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Influence>, AppError> {
    let mut influence = state
        .db
        .update_influence_type(auth_data.user_id, path.influenced_to, path.type_id)
        .await?;

    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut influence.beatmaps,
    )
    .await?;
    Ok(Json(influence))
}

pub async fn get_user_mentions(
    Query(pagination): Query<PaginationQuery>,
    Path(user_id): Path<PathUserId>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Influence>>, AppError> {
    let mentions = state
        .db
        .get_mentions(user_id.value, pagination.start, pagination.limit)
        .await?;
    Ok(Json(mentions))
}

pub async fn get_user_influences(
    Query(pagination): Query<PaginationQuery>,
    Path(user_id): Path<PathUserId>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Influence>>, AppError> {
    let mut influences = state
        .db
        .get_influences(user_id.value, pagination.start, pagination.limit)
        .await?;

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
