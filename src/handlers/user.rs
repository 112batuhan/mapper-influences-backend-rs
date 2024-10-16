use std::{collections::HashSet, sync::Arc};

use axum::{
    extract::{Path, State},
    Extension, Json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    database::user::UserDb,
    error::AppError,
    jwt::AuthData,
    osu_api::{Group, OsuBeatmapCondensed, OsuMultipleBeatmapResponse, OsuMultipleUserResponse},
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

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Test {
    users: Vec<u32>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UserResponse {
    pub id: u32,
    pub username: String,
    pub avatar_url: String,
    pub bio: String,
    pub mention_count: u32,
    pub groups: Vec<Group>,
    pub country_code: String,
    pub country_name: String,
    pub previous_usernames: Vec<String>,
    pub ranked_and_approved_beatmapset_count: u32,
    pub ranked_beatmapset_count: u32,
    pub nominated_beatmapset_count: u32,
    pub guest_beatmapset_count: u32,
    pub loved_beatmapset_count: u32,
    pub graveyard_beatmapset_count: u32,
    pub pending_beatmapset_count: u32,
    pub beatmaps: Vec<OsuBeatmapCondensed>,
}

pub async fn test(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    Json(test): Json<Test>,
) -> Result<(), AppError> {
    let users = state
        .osu_beatmap_multi_requester
        .get_multiple_osu(&test.users, &auth_data.osu_token)
        .await?;
    dbg!(users);
    Ok(())
}

pub async fn user_data_handle(
    state: Arc<AppState>,
    osu_token: String,
    user_data: UserDb,
) -> Result<UserResponse, AppError> {
    let beatmaps: Vec<OsuMultipleBeatmapResponse> = state
        .osu_beatmap_multi_requester
        .get_multiple_osu(&user_data.beatmaps, &osu_token)
        .await?
        .into_values()
        .collect();

    // usually users add their own maps to showcase, to skip on user request, we first remove the
    // requested user from the list, then add it back while adding user data to the beatmaps.
    // we could add one more cache layer and pull data from database before going for user requests
    // but this is already saving on requests massively. No need to premature optimization.
    // plus I didn't like the performance of the surrealdb, so it's better to split the load.
    let mut users_needed: HashSet<u32> = beatmaps.iter().map(|beatmap| beatmap.user_id).collect();
    users_needed.remove(&user_data.id);
    let users_needed: Vec<u32> = users_needed.into_iter().collect();
    let mut users = state
        .osu_user_multi_requester
        .get_multiple_osu(&users_needed, &osu_token)
        .await?;
    users.insert(
        user_data.id,
        OsuMultipleUserResponse {
            id: user_data.id,
            avatar_url: user_data.avatar_url.clone(),
            username: user_data.username.clone(),
        },
    );

    let beatmaps = beatmaps
        .into_iter()
        .filter_map(|beatmap| {
            //NOTE: Possible fail point, properly handle errors
            //there could be missing maps but extremely unlikely
            let user = users.get(&beatmap.user_id)?;
            Some(OsuBeatmapCondensed::from_osu_multiple_and_user_data(
                beatmap,
                user.username.clone(),
                user.avatar_url.clone(),
            ))
        })
        .collect();

    Ok(UserResponse {
        id: user_data.id,
        username: user_data.username,
        avatar_url: user_data.avatar_url,
        bio: user_data.bio,
        mention_count: user_data.mention_count,
        country_code: user_data.country_code,
        country_name: user_data.country_name,
        groups: user_data.groups,
        previous_usernames: user_data.previous_usernames,
        ranked_and_approved_beatmapset_count: user_data.ranked_and_approved_beatmapset_count,
        ranked_beatmapset_count: user_data.ranked_beatmapset_count,
        nominated_beatmapset_count: user_data.nominated_beatmapset_count,
        guest_beatmapset_count: user_data.guest_beatmapset_count,
        loved_beatmapset_count: user_data.loved_beatmapset_count,
        graveyard_beatmapset_count: user_data.graveyard_beatmapset_count,
        pending_beatmapset_count: user_data.pending_beatmapset_count,
        beatmaps,
    })
}

pub async fn get_me(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<UserResponse>, AppError> {
    let user_data = state.db.get_user_details(auth_data.user_id).await?;
    let complete_user_data = user_data_handle(state, auth_data.osu_token, user_data).await?;
    Ok(Json(complete_user_data))
}

pub async fn get_user(
    Extension(auth_data): Extension<AuthData>,
    Path(user_id): Path<u32>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<UserResponse>, AppError> {
    let user_data = state.db.get_user_details(user_id).await?;
    let complete_user_data = user_data_handle(state, auth_data.osu_token, user_data).await?;
    Ok(Json(complete_user_data))
}

pub async fn get_user_without_auth(
    Path(user_id): Path<u32>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<UserDb>, AppError> {
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
        .osu_beatmap_multi_requester
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
