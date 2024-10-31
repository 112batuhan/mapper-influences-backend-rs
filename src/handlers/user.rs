use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Extension, Json,
};
use itertools::Itertools;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    database::user::User,
    error::AppError,
    jwt::AuthData,
    osu_api::{cached_osu_user_request, BeatmapEnum, GetID},
    AppState,
};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Bio {
    pub bio: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Order {
    pub influence_ids: Vec<u32>,
}

async fn user_data_handle(
    state: Arc<AppState>,
    osu_token: String,
    mut user: User,
) -> Result<User, AppError> {
    let beatmaps_to_request: Vec<u32> = user
        .beatmaps
        .iter()
        .map(|map| map.get_id())
        .unique()
        .collect();

    let mut beatmaps = state
        .cached_combined_requester
        .clone()
        .get_beatmaps_with_user(&beatmaps_to_request, &osu_token)
        .await?;

    // to keep the order, we iterate user beatmaps
    let new_beatmaps = user
        .beatmaps
        .iter()
        .filter_map(|beatmap| {
            // remove should be ok, we keep beatmaps as set in db, so they should be unique
            let beatmap = beatmaps.remove(&beatmap.get_id())?;
            Some(BeatmapEnum::All(beatmap))
        })
        .collect();

    user.beatmaps = new_beatmaps;
    Ok(user)
}

pub async fn get_me(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<User>, AppError> {
    let user_data = state.db.get_user_details(auth_data.user_id).await?;
    let complete_user_data = user_data_handle(state, auth_data.osu_token, user_data).await?;
    Ok(Json(complete_user_data))
}

/// Returns a database user, If the user is not in database, then returns a osu! API response
pub async fn get_user(
    Extension(auth_data): Extension<AuthData>,
    Path(user_id): Path<u32>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<User>, AppError> {
    let user_result = state.db.get_user_details(user_id).await;

    let user_data = match user_result {
        // Early return without any processing if the user is not in DB
        Err(AppError::MissingUser(_)) => {
            let user_osu =
                cached_osu_user_request(state.request.clone(), &auth_data.osu_token, user_id)
                    .await?;
            return Ok(Json(user_osu.into()));
        }
        Err(error) => return Err(error),
        Ok(data) => data,
    };

    let complete_user_data = user_data_handle(state, auth_data.osu_token, user_data).await?;
    Ok(Json(complete_user_data))
}

pub async fn get_user_without_auth(
    Path(user_id): Path<u32>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<User>, AppError> {
    let user_data = state.db.get_user_details(user_id).await?;
    Ok(Json(user_data))
}

pub async fn update_user_bio(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(bio): Json<Bio>,
) -> Result<(), AppError> {
    state.db.update_bio(auth_data.user_id, bio.bio).await?;
    Ok(())
}

pub async fn add_user_beatmap(
    Path(beatmap_id): Path<u32>,
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
        .add_beatmap_to_user(auth_data.user_id, beatmap_id)
        .await?;
    Ok(())
}

pub async fn delete_user_beatmap(
    Path(beatmap_id): Path<u32>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<(), AppError> {
    state
        .db
        .remove_beatmap_from_user(auth_data.user_id, beatmap_id)
        .await?;
    Ok(())
}

pub async fn set_influence_order(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(order_request): Json<Order>,
) -> Result<(), AppError> {
    state
        .db
        .set_influence_order(auth_data.user_id, &order_request.influence_ids)
        .await?;
    Ok(())
}
