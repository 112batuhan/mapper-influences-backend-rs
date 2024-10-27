use axum::{
    extract::{Path, State},
    Extension, Json,
};
use futures::try_join;
use itertools::Itertools;
use schemars::JsonSchema;
use serde::Deserialize;
use std::{collections::HashSet, sync::Arc};

use crate::{
    database::influence::Influence,
    error::AppError,
    jwt::AuthData,
    osu_api::{BeatmapEnum, OsuBeatmapSmall, OsuMultipleUser},
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
        .beatmap_requester
        .clone()
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
) -> Result<Json<Vec<Influence>>, AppError> {
    let mentions = state.db.get_mentions(user_id).await?;
    Ok(Json(mentions))
}

pub async fn get_user_influences(
    Path(user_id): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Influence>>, AppError> {
    let influences = state.db.get_influences(user_id).await?;
    let beatmaps_to_request: Vec<u32> = influences
        .iter()
        .flat_map(|influence| &influence.beatmaps)
        .filter_map(|maps| match maps {
            BeatmapEnum::Id(id) => Some(id),
            BeatmapEnum::All(_) => None,
        })
        .unique()
        .copied()
        .collect();
    // Request beatmaps to populate beatmap data
    let beatmaps = state
        .beatmap_requester
        .clone()
        .get_multiple_osu(&beatmaps_to_request, &auth_data.osu_token)
        .await?;
    // Get a list of users to request. Users that got queried in db is excluded and will be added
    // back to the hashmap that contains the user data.
    let mut users_to_request: HashSet<u32> = beatmaps.values().map(|map| map.user_id).collect();
    influences.iter().for_each(|influence| {
        users_to_request.remove(&influence.user_id);
    });
    let users_to_request: Vec<u32> = users_to_request.into_iter().collect();
    // Users queried
    let mut users = state
        .user_requester
        .clone()
        .get_multiple_osu(&users_to_request, &auth_data.osu_token)
        .await?;
    // DB users are inserted back to the user map
    users.extend(influences.iter().map(|mention| {
        (
            mention.user_id,
            OsuMultipleUser {
                id: mention.user_id,
                avatar_url: mention.avatar_url.clone(),
                username: mention.username.clone(),
            },
        )
    }));
    // Influences converted with beatmap data
    let influences = influences
        .into_iter()
        .map(|mut influence| {
            let beatmaps: Vec<BeatmapEnum> = influence
                .beatmaps
                .into_iter()
                .filter_map(|maps| match maps {
                    BeatmapEnum::Id(id) => Some(id),
                    BeatmapEnum::All(_) => None,
                })
                // TODO: Maybe there is a way to refactor this
                // we have the same thing going on in user handler
                .filter_map(|beatmap| {
                    //NOTE: Possible fail point, properly handle errors
                    //there could be missing maps but extremely unlikely
                    let beatmap = beatmaps.get(&beatmap)?;
                    let user = users.get(&beatmap.user_id)?;
                    let beatmap_small = OsuBeatmapSmall::from_osu_beatmap_and_user_data(
                        beatmap.clone(),
                        user.username.clone(),
                        user.avatar_url.clone(),
                    );
                    Some(BeatmapEnum::All(beatmap_small))
                })
                .collect();
            influence.beatmaps = beatmaps;
            influence
        })
        .collect();

    Ok(Json(influences))
}
