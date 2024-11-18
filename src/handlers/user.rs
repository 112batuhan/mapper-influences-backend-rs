use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Extension, Json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    database::user::User, error::AppError, jwt::AuthData,
    osu_api::cached_requester::cached_osu_user_request, AppState,
};

use super::{check_multiple_maps, swap_beatmaps, BeatmapRequest, PathBeatmapId, PathUserId};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Bio {
    pub bio: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Order {
    pub influence_user_ids: Vec<u32>,
}

pub async fn get_me(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<User>, AppError> {
    let mut user = state.db.get_user_details(auth_data.user_id).await?;
    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut user.beatmaps,
    )
    .await?;
    Ok(Json(user))
}

/// Returns a database user, If the user is not in database, then returns an osu! API response
pub async fn get_user(
    Extension(auth_data): Extension<AuthData>,
    Path(user_id): Path<PathUserId>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<User>, AppError> {
    let user_result = state.db.get_user_details(user_id.value).await;

    let mut user = match user_result {
        // Early return without any processing if the user is not in DB
        Err(AppError::MissingUser(_)) => {
            let user_osu =
                cached_osu_user_request(state.request.clone(), &auth_data.osu_token, user_id.value)
                    .await?;
            return Ok(Json(user_osu.into()));
        }
        Err(error) => return Err(error),
        Ok(data) => data,
    };

    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut user.beatmaps,
    )
    .await?;
    Ok(Json(user))
}

pub async fn update_user_bio(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(bio): Json<Bio>,
) -> Result<Json<User>, AppError> {
    const MAX_BIO_LENGTH: usize = 5000;
    if bio.bio.len() > MAX_BIO_LENGTH {
        return Err(AppError::StringTooLong);
    }
    let mut user = state.db.update_bio(auth_data.user_id, bio.bio).await?;
    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut user.beatmaps,
    )
    .await?;
    Ok(Json(user))
}

pub async fn add_user_beatmap(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(beatmaps): Json<BeatmapRequest>,
) -> Result<Json<User>, AppError> {
    let beatmaps: Vec<u32> = beatmaps.ids.into_iter().collect();
    check_multiple_maps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &beatmaps,
    )
    .await?;

    let mut user = state
        .db
        .add_beatmap_to_user(auth_data.user_id, beatmaps)
        .await?;
    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut user.beatmaps,
    )
    .await?;
    Ok(Json(user))
}

pub async fn delete_user_beatmap(
    Path(beatmap_id): Path<PathBeatmapId>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<User>, AppError> {
    let mut user = state
        .db
        .remove_beatmap_from_user(auth_data.user_id, beatmap_id.value)
        .await?;
    swap_beatmaps(
        state.cached_combined_requester.clone(),
        &auth_data.osu_token,
        &mut user.beatmaps,
    )
    .await?;
    Ok(Json(user))
}

pub async fn set_influence_order(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(order_request): Json<Order>,
) -> Result<(), AppError> {
    state
        .db
        .set_influence_order(auth_data.user_id, &order_request.influence_user_ids)
        .await?;
    Ok(())
}
