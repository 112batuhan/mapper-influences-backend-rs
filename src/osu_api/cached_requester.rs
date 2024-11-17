use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use cached::proc_macro::cached;
use itertools::Itertools;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{custom_cache::CustomCache, error::AppError};

use super::{
    request::Requester, GetID, OsuBeatmapSmall, OsuMultipleBeatmap, OsuMultipleUser, UserOsu,
};

pub struct CachedRequester<T: DeserializeOwned + GetID + Clone + Send + 'static> {
    pub client: Arc<dyn Requester>,
    pub cache: Mutex<CustomCache<u32, T>>,
    pub base_url: String,
}

impl<T: DeserializeOwned + GetID + Clone + Send + 'static> CachedRequester<T> {
    pub fn new(
        client: Arc<dyn Requester>,
        base_url: &str,
        cache_expiration: u32,
    ) -> CachedRequester<T> {
        CachedRequester {
            client,
            cache: Mutex::new(CustomCache::new(cache_expiration)),
            base_url: base_url.to_string(),
        }
    }

    pub async fn get_multiple_osu(
        self: Arc<Self>,
        ids: &[u32],
        access_token: &str,
    ) -> Result<HashMap<u32, T>, AppError> {
        // try to get the results from cache
        let mut cache_result = {
            let mut cache = self.cache.lock().map_err(|_| AppError::Mutex)?;
            cache.get_multiple(ids)
        };
        // Request the missing items
        let misses_requested = self
            .client
            .clone()
            .request_multiple(&self.base_url, &cache_result.misses, access_token)
            .await?;

        let misses_requested: Vec<T> = serde_json::from_value(Value::Array(misses_requested))?;

        // Map the results to add to cache
        let add_to_cache: Vec<(u32, T)> = misses_requested
            .into_iter()
            .map(|value| (value.get_id(), value))
            .collect();

        // Update the cache with the new data
        {
            let mut cache = self.cache.lock().map_err(|_| AppError::Mutex)?;
            cache.set_multiple(add_to_cache.clone());
        }

        // Combine hits with newly fetched data
        cache_result.hits.extend(add_to_cache.into_iter());

        Ok(cache_result.hits)
    }
}

pub struct CombinedRequester {
    user_requester: Arc<CachedRequester<OsuMultipleUser>>,
    beatmap_requester: Arc<CachedRequester<OsuMultipleBeatmap>>,
}
impl CombinedRequester {
    pub fn new(client: Arc<dyn Requester>, base_url: &str) -> Arc<Self> {
        let user_requester = Arc::new(CachedRequester::new(
            client.clone(),
            &format!("{}/api/v2/users", base_url),
            24600,
        ));
        let beatmap_requester = Arc::new(CachedRequester::new(
            client.clone(),
            &format!("{}/api/v2/beatmaps", base_url),
            86400,
        ));
        Arc::new(CombinedRequester {
            user_requester,
            beatmap_requester,
        })
    }

    pub async fn get_beatmaps_with_user(
        &self,
        ids: &[u32],
        access_token: &str,
    ) -> Result<HashMap<u32, OsuBeatmapSmall>, AppError> {
        let beatmap_map = self
            .beatmap_requester
            .clone()
            .get_multiple_osu(ids, access_token)
            .await?;
        let users_to_request: Vec<u32> = beatmap_map
            .values()
            .map(|beatmap| beatmap.user_id)
            .unique()
            .collect();
        let user_map = self
            .user_requester
            .clone()
            .get_multiple_osu(&users_to_request, access_token)
            .await?;
        let combined = beatmap_map
            .into_iter()
            .map(|(beatmap_id, beatmap)| {
                let user = user_map.get(&beatmap.user_id).cloned();
                let new_beatmap = OsuBeatmapSmall::from_osu_beatmap_and_user_data(beatmap, user);
                (beatmap_id, new_beatmap)
            })
            .collect();

        Ok(combined)
    }

    pub async fn get_beatmaps_only(
        &self,
        ids: &[u32],
        access_token: &str,
    ) -> Result<HashMap<u32, OsuMultipleBeatmap>, AppError> {
        let beatmap_map = self
            .beatmap_requester
            .clone()
            .get_multiple_osu(ids, access_token)
            .await?;
        Ok(beatmap_map)
    }
    pub async fn get_users_only(
        &self,
        ids: &[u32],
        access_token: &str,
    ) -> Result<HashMap<u32, OsuMultipleUser>, AppError> {
        let user_map = self
            .user_requester
            .clone()
            .get_multiple_osu(ids, access_token)
            .await?;
        Ok(user_map)
    }
}

#[cached(
    ty = "CustomCache<u32, UserOsu>",
    create = "{CustomCache::new(21600)}",
    convert = r#"{user_id}"#,
    result = true
)]
pub async fn cached_osu_user_request(
    client: Arc<dyn Requester>,
    osu_token: &str,
    user_id: u32,
) -> Result<UserOsu, AppError> {
    let user_osu = client.get_user_osu(osu_token, user_id).await?;
    Ok(user_osu)
}
