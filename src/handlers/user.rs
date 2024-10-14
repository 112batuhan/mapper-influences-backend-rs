use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use crate::{database::user::UserDb, error::AppError, jwt::AuthData, AppState};

#[derive(Serialize, Deserialize)]
pub struct Bio {
    pub bio: String,
}

#[derive(Serialize, Deserialize)]
pub struct Order {
    pub influence_ids: Vec<u32>,
}

pub async fn get_me(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<UserDb>, AppError> {
    let user_data = state.db.get_user_details(auth_data.user_id).await?;
    Ok(Json(user_data))
}

pub async fn get_user(
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
