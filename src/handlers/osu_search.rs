use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    Extension, Json,
};
use itertools::Itertools;

use crate::{
    database::user::UserSmall,
    error::AppError,
    jwt::AuthData,
    osu_api::{cached_requester::cached_osu_user_request, BeatmapsetSmall},
    AppState,
};

use super::{PathBeatmapId, PathQuery};

const USER_SEARCH_EXPIRATION: u64 = 600;
const BEATMAP_SEARCH_EXPIRATION: u64 = 300;

pub async fn osu_user_search(
    Path(path_query): Path<PathQuery>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<UserSmall>>, AppError> {
    let cache_key = format!("search:user:{}", path_query.value);
    if let Some(users) = state.cache.get::<Vec<UserSmall>>(&cache_key).await {
        return Ok(Json(users));
    }

    let user_search_osu = state
        .request
        .search_user_osu(&auth_data.osu_token, &path_query.value)
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
        let cache = state.cache.clone();
        let osu_token = auth_data.osu_token.to_string();
        let handle =
            tokio::spawn(
                async move { cached_osu_user_request(client, cache, &osu_token, id).await },
            );
        handles.push(handle);
    }

    for handle in handles {
        if let Ok(request_result) = handle.await {
            users.push(request_result?.into())
        }
    }

    state
        .cache
        .set(&cache_key, &users, USER_SEARCH_EXPIRATION)
        .await;
    Ok(Json(users))
}

pub async fn osu_beatmap_search(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Result<Json<Vec<BeatmapsetSmall>>, AppError> {
    let uri = request.uri().to_string();
    let cache_key = format!("search:beatmap:{}", uri);
    if let Some(beatmaps) = state.cache.get::<Vec<BeatmapsetSmall>>(&cache_key).await {
        return Ok(Json(beatmaps));
    }

    let query = uri
        .strip_prefix("/search/map?")
        .ok_or(AppError::BadUri(uri.clone()))?;
    let beatmap_search_osu = state
        .request
        .search_map_osu(&auth_data.osu_token, query)
        .await?;

    let users_to_request: Vec<u32> = beatmap_search_osu
        .beatmapsets
        .iter()
        .map(|beatmapset| beatmapset.user_id)
        .unique()
        .collect();

    let user_map = state
        .cached_combined_requester
        .get_users_only(&users_to_request, &auth_data.osu_token)
        .await?;

    let beatmap_search: Vec<BeatmapsetSmall> = beatmap_search_osu
        .beatmapsets
        .into_iter()
        .map(|beatmapset| {
            let user = user_map.get(&beatmapset.user_id).cloned();
            BeatmapsetSmall::from_base_beapmapset_and_user(beatmapset, user)
        })
        .collect();

    state
        .cache
        .set(&cache_key, &beatmap_search, BEATMAP_SEARCH_EXPIRATION)
        .await;
    Ok(Json(beatmap_search))
}

pub async fn osu_singular_beatmap_serch(
    Path(beatmap_path): Path<PathBeatmapId>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<BeatmapsetSmall>, AppError> {
    let beatmap_map = state
        .cached_combined_requester
        .clone()
        .get_beatmaps_with_user(&[beatmap_path.value], &auth_data.osu_token)
        .await?;

    beatmap_map
        .into_values()
        .map(Json)
        .next()
        .ok_or(AppError::NonExistingMap(beatmap_path.value))
}
