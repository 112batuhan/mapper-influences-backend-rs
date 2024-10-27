use std::{collections::HashSet, sync::Arc};

use axum::{
    extract::{Path, State},
    Extension, Json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    database::user::User,
    error::AppError,
    jwt::AuthData,
    osu_api::{cached_osu_user_request, BeatmapEnum, OsuBeatmapSmall, OsuMultipleUser},
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

pub async fn user_data_handle(
    state: Arc<AppState>,
    osu_token: String,
    mut user: User,
) -> Result<User, AppError> {
    let beatmaps_to_request: Vec<u32> = user
        .beatmaps
        .iter()
        .filter_map(|map| match map {
            BeatmapEnum::Id(id) => Some(id),
            BeatmapEnum::All(_) => None,
        })
        .copied()
        .collect();
    let beatmaps = state
        .beatmap_requester
        .clone()
        .get_multiple_osu(&beatmaps_to_request, &osu_token)
        .await?;

    // Get a list of users to request. User that got queried with the db will be put
    // back to the hashmap that contains the user data.
    let mut users_needed: HashSet<u32> = beatmaps.values().map(|beatmap| beatmap.user_id).collect();
    users_needed.remove(&user.id);
    let users_needed: Vec<u32> = users_needed.into_iter().collect();

    // users queried
    let mut users = state
        .user_requester
        .clone()
        .get_multiple_osu(&users_needed, &osu_token)
        .await?;

    // Db user put back to the user map
    users.insert(
        user.id,
        OsuMultipleUser {
            id: user.id,
            avatar_url: user.avatar_url.clone(),
            username: user.username.clone(),
        },
    );

    // beatmaps populated with user data
    let beatmaps: Vec<BeatmapEnum> = beatmaps
        .into_values()
        .filter_map(|beatmap| {
            //NOTE: Possible fail point, properly handle errors
            //there could be missing maps but extremely unlikely
            let user = users.get(&beatmap.user_id)?;
            let beatmap_small = OsuBeatmapSmall::from_osu_beatmap_and_user_data(
                beatmap,
                user.username.clone(),
                user.avatar_url.clone(),
            );
            Some(BeatmapEnum::All(beatmap_small))
        })
        .collect();

    user.beatmaps = beatmaps;
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
        .beatmap_requester
        .clone()
        .get_multiple_osu(&[beatmap_id], &auth_data.osu_token)
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
