use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    Extension, Json,
};
use cached::proc_macro::cached;
use itertools::Itertools;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    custom_cache::CustomCache,
    database::user::UserSmall,
    error::AppError,
    jwt::AuthData,
    osu_api::{
        cached_requester::cached_osu_user_request, BaseBeatmapset, BeatmapOsu, OsuMultipleUser,
    },
    AppState,
};

use super::PathQuery;

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
/// `SearchBeatmapset` type. For more compact beatmap search results
pub struct SearchBeatmapset {
    pub id: u32,
    pub beatmaps: Vec<BeatmapOsu>,
    pub title: String,
    pub artist: String,
    pub cover: String,
    pub user_name: String,
    pub user_avatar_url: String,
    pub user_id: u32,
}

impl SearchBeatmapset {
    /// This function combines [`BaseBeatmapset`] and [`OsuMultipleUser`].
    ///
    /// If user is not returned from the query, we fallback to beatmapset user.
    /// This usually happens if the original mapper is banned. If the beatmapset submitter is also
    /// banned, we don't have to worry about the avatar_url as osu automatically falls back to
    /// guest picture.
    pub fn from_base_beapmapset_and_user(
        api_set: BaseBeatmapset,
        user_multiple: Option<OsuMultipleUser>,
    ) -> Self {
        let user_name: String;
        let user_avatar_url: String;

        if let Some(user_multiple) = user_multiple {
            user_name = user_multiple.username;
            user_avatar_url = user_multiple.avatar_url;
        } else {
            user_name = api_set.creator;
            user_avatar_url = format!("https://a.ppy.sh/{}?", api_set.user_id);
        }

        SearchBeatmapset {
            id: api_set.id,
            beatmaps: api_set.beatmaps,
            title: api_set.title,
            artist: api_set.artist,
            cover: api_set.covers.cover,
            user_id: api_set.user_id,
            user_name,
            user_avatar_url,
        }
    }
}

#[cached(
    ty = "CustomCache<String, Json<Vec<UserSmall>>>",
    create = "{CustomCache::new(600)}",
    convert = r#"{path_query.value.clone()}"#,
    result = true
)]
pub async fn osu_user_search(
    Path(path_query): Path<PathQuery>,
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<UserSmall>>, AppError> {
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
    ty = "CustomCache<String, Json<Vec<SearchBeatmapset>>>",
    create = "{CustomCache::new(300)}",
    convert = r#"{request.uri().to_string()}"#,
    result = true
)]
pub async fn osu_beatmap_search(
    Extension(auth_data): Extension<AuthData>,
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Result<Json<Vec<SearchBeatmapset>>, AppError> {
    let uri = request.uri().to_string();
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

    let beatmap_search = beatmap_search_osu
        .beatmapsets
        .into_iter()
        .map(|beatmapset| {
            let user = user_map.get(&beatmapset.user_id).cloned();
            SearchBeatmapset::from_base_beapmapset_and_user(beatmapset, user)
        })
        .collect();

    Ok(Json(beatmap_search))
}
