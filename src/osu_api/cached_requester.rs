use std::{collections::HashMap, marker::PhantomData, sync::Arc};

use itertools::Itertools;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::{
    cache::{MultipleCacheResults, RedisCache},
    error::AppError,
};

use super::{
    request::Requester, BeatmapsetSmall, GetID, OsuMultipleBeatmap, OsuMultipleUser, UserOsu,
};

const USER_EXPIRATION: u64 = 24600;
const BEATMAP_EXPIRATION: u64 = 86400;
const FULL_USER_EXPIRATION: u64 = 21600;

const USER_KEY_PREFIX: &str = "osu:multiple_user:";
const BEATMAP_KEY_PREFIX: &str = "osu:multiple_beatmap:";
const FULL_USER_KEY_PREFIX: &str = "osu:user:";

pub struct CachedRequester<T: DeserializeOwned + Serialize + GetID + Clone + Send + 'static> {
    pub client: Arc<dyn Requester>,
    pub cache: Arc<RedisCache>,
    pub base_url: String,
    pub key_prefix: &'static str,
    pub expire_in: u64,
    value: PhantomData<fn() -> T>,
}

impl<T: DeserializeOwned + Serialize + GetID + Clone + Send + 'static> CachedRequester<T> {
    pub fn new(
        client: Arc<dyn Requester>,
        cache: Arc<RedisCache>,
        base_url: &str,
        key_prefix: &'static str,
        expire_in: u64,
    ) -> CachedRequester<T> {
        CachedRequester {
            client,
            cache,
            base_url: base_url.to_string(),
            key_prefix,
            expire_in,
            value: PhantomData,
        }
    }

    pub async fn get_multiple_osu(
        self: Arc<Self>,
        ids: &[u32],
        access_token: &str,
    ) -> Result<HashMap<u32, T>, AppError> {
        // try to get the results from cache
        let mut cache_result: MultipleCacheResults<u32, T> =
            self.cache.get_multiple(self.key_prefix, ids).await?;

        if cache_result.misses.is_empty() {
            return Ok(cache_result.hits);
        }

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
        self.cache
            .set_multiple(self.key_prefix, &add_to_cache, self.expire_in)
            .await?;

        // Combine hits with newly fetched data
        cache_result.hits.extend(add_to_cache);

        Ok(cache_result.hits)
    }
}

pub struct CombinedRequester {
    user_requester: Arc<CachedRequester<OsuMultipleUser>>,
    beatmap_requester: Arc<CachedRequester<OsuMultipleBeatmap>>,
}
impl CombinedRequester {
    pub fn new(client: Arc<dyn Requester>, cache: Arc<RedisCache>, base_url: &str) -> Arc<Self> {
        let user_requester = Arc::new(CachedRequester::new(
            client.clone(),
            cache.clone(),
            &format!("{}/api/v2/users", base_url),
            USER_KEY_PREFIX,
            USER_EXPIRATION,
        ));
        let beatmap_requester = Arc::new(CachedRequester::new(
            client.clone(),
            cache,
            &format!("{}/api/v2/beatmaps", base_url),
            BEATMAP_KEY_PREFIX,
            BEATMAP_EXPIRATION,
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
    ) -> Result<HashMap<u32, BeatmapsetSmall>, AppError> {
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
                let new_beatmap = BeatmapsetSmall::from_osu_beatmap_and_user_data(beatmap, user);
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

pub async fn cached_osu_user_request(
    client: Arc<dyn Requester>,
    cache: Arc<RedisCache>,
    osu_token: &str,
    user_id: u32,
) -> Result<UserOsu, AppError> {
    let cache_key = format!("{}{}", FULL_USER_KEY_PREFIX, user_id);
    if let Some(user_osu) = cache.get::<UserOsu>(&cache_key).await? {
        return Ok(user_osu);
    }

    let user_osu = client.get_user_osu(osu_token, user_id).await?;
    cache
        .set(&cache_key, &user_osu, FULL_USER_EXPIRATION)
        .await?;
    Ok(user_osu)
}
