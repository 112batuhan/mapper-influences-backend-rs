use axum::{
    extract::{Path, State},
    Extension, Json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

use crate::{
    database::influence::{InfluenceDb, InfluenceWithoutBeatmaps},
    error::AppError,
    jwt::AuthData,
    osu_api::{OsuBeatmapCondensed, OsuMultipleUserResponse},
    AppState,
};

#[derive(Deserialize, JsonSchema)]
pub struct Description {
    description: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InfluenceResponse {
    #[serde(flatten)]
    data: InfluenceWithoutBeatmaps,
    beatmaps: Vec<OsuBeatmapCondensed>,
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
) -> Result<Json<Vec<InfluenceDb>>, AppError> {
    let mentions = state.db.get_mentions(user_id).await?;
    Ok(Json(mentions))
}

pub async fn get_user_influences(
    Path(user_id): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InfluenceResponse>>, AppError> {
    let influences = state.db.get_influences(user_id).await?;
    let beatmaps_to_request: Vec<u32> = influences
        .iter()
        .flat_map(|influence| &influence.beatmaps)
        .copied()
        .collect();
    // Request beatmaps to populate beatmap data
    let beatmaps = state
        .osu_beatmap_multi_requester
        .get_multiple_osu(&beatmaps_to_request, &auth_data.osu_token)
        .await?;
    // Get a list of users to request. Users that got queried in db is excluded and will be added
    // back to the hashmap that contains the user data.
    let mut users_to_request: HashSet<u32> = beatmaps.values().map(|map| map.user_id).collect();
    influences.iter().for_each(|influence| {
        users_to_request.remove(&influence.data.id);
    });
    let users_to_request: Vec<u32> = users_to_request.into_iter().collect();
    // Users queried
    let mut users = state
        .osu_user_multi_requester
        .get_multiple_osu(&users_to_request, &auth_data.osu_token)
        .await?;
    // DB users are inserted back to the user map
    users.extend(influences.iter().map(|mention| {
        (
            mention.data.id,
            OsuMultipleUserResponse {
                id: mention.data.id,
                avatar_url: mention.data.avatar_url.clone(),
                username: mention.data.username.clone(),
            },
        )
    }));
    // Influences converted with beatmap data
    let influences = influences
        .into_iter()
        .map(|influence| {
            let beatmaps: Vec<OsuBeatmapCondensed> = influence
                .beatmaps
                .into_iter()
                .filter_map(|beatmap| {
                    //NOTE: Possible fail point, properly handle errors
                    //there could be missing maps but extremely unlikely
                    let beatmap = beatmaps.get(&beatmap)?;
                    let user = users.get(&beatmap.user_id)?;
                    Some(OsuBeatmapCondensed::from_osu_multiple_and_user_data(
                        beatmap.clone(),
                        user.username.clone(),
                        user.avatar_url.clone(),
                    ))
                })
                .collect();
            InfluenceResponse {
                data: influence.data,
                beatmaps,
            }
        })
        .collect();

    Ok(Json(influences))
}
