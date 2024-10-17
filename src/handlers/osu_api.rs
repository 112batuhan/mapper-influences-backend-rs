use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use cached::proc_macro::cached;

use crate::{
    custom_cache::CustomCache,
    database::user::UserCondensed,
    error::AppError,
    jwt::AuthData,
    osu_api::{cached_osu_user_request, OsuSearchMapResponse},
    AppState,
};

#[cached(
    ty = "CustomCache<String, Json<Vec<UserCondensed>>>",
    create = "{CustomCache::new(600)}",
    convert = r#"{query.clone()}"#,
    result = true
)]
pub async fn osu_user_search(
    Path(query): Path<String>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<UserCondensed>>, AppError> {
    let user_search_osu = state
        .request
        .search_user_osu(&auth_data.osu_token, &query)
        .await?
        .user
        .data;

    let mut users_to_get: Vec<u32> = user_search_osu
        .into_iter()
        .take(3)
        .map(|user_id| user_id.id)
        .collect();

    let mut users = state.db.get_multiple_user_details(&users_to_get).await?;

    let db_user_ids: Vec<u32> = users.iter().map(|user| user.id).collect();
    users_to_get.retain(|id| !db_user_ids.contains(id));

    let mut handles = Vec::new();
    for id in users_to_get {
        let client = state.request.clone();
        let osu_token = auth_data.osu_token.to_string();
        let handle =
            tokio::spawn(async move { cached_osu_user_request(client, &osu_token, id).await });
        handles.push(handle);
    }

    for handle in handles {
        if let Ok(request_result) = handle.await {
            users.push(request_result?.into())
        }
    }

    Ok(Json(users))
}

#[cached(
    ty = "CustomCache<String, Json<OsuSearchMapResponse>>",
    create = "{CustomCache::new(600)}",
    convert = r#"{query.clone()}"#,
    result = true
)]
pub async fn osu_beatmap_search(
    Query(query): Query<String>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<OsuSearchMapResponse>, AppError> {
    let beatmap_search_osu = state
        .request
        .search_map_osu(&auth_data.osu_token, &query)
        .await?;
    Ok(Json(beatmap_search_osu))
}
