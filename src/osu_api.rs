use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, RwLock},
    time::Duration,
};

use cached::proc_macro::cached;
use futures::{future::try_join_all, Future};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use tokio::{sync::Semaphore, time::sleep};
use tracing::error;

use crate::{
    custom_cache::CustomCache,
    error::AppError,
    retry::{Retry, RetryAction, RetryOption, Retryable},
};

static CLIENT_ID: LazyLock<String> =
    LazyLock::new(|| std::env::var("CLIENT_ID").expect("Missing CLIENT_ID environment variable"));

static CLIENT_SECRET: LazyLock<String> = LazyLock::new(|| {
    std::env::var("CLIENT_SECRET").expect("Missing CLIENT_SECRET environment variable")
});

static REDIRECT_URI: LazyLock<String> = LazyLock::new(|| {
    std::env::var("REDIRECT_URI").expect("Missing REDIRECT_URI environment variable")
});

pub trait GetID {
    fn get_id(&self) -> u32;
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct UserId {
    pub id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuMultipleUser {
    pub id: u32,
    pub avatar_url: String,
    pub username: String,
}
impl GetID for OsuMultipleUser {
    fn get_id(&self) -> u32 {
        self.id
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct Country {
    pub code: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq, Eq)]
pub struct Group {
    pub colour: Option<String>,
    pub name: String,
    pub short_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct UserOsu {
    pub id: u32,
    pub username: String,
    pub avatar_url: String,
    pub country: Country,
    pub groups: Vec<Group>,
    pub previous_usernames: Vec<String>,
    pub ranked_and_approved_beatmapset_count: u32,
    pub ranked_beatmapset_count: u32,
    pub nominated_beatmapset_count: u32,
    pub guest_beatmapset_count: u32,
    pub loved_beatmapset_count: u32,
    pub graveyard_beatmapset_count: u32,
    pub pending_beatmapset_count: u32,
}
impl UserOsu {
    pub fn is_ranked_mapper(&self) -> bool {
        self.ranked_beatmapset_count + self.loved_beatmapset_count + self.guest_beatmapset_count > 0
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuSearchUserData {
    pub data: Vec<UserId>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuSearchUserResponse {
    pub user: OsuSearchUserData,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct BeatmapOsu {
    pub difficulty_rating: f64,
    pub id: u32,
    pub mode: String,
    pub beatmapset_id: u32,
    pub version: String,
}

impl GetID for BeatmapOsu {
    fn get_id(&self) -> u32 {
        self.id
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct BeatmapsetRelatedUser {
    pub username: String,
    pub avatar_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct Cover {
    pub cover: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct BaseBeatmapset {
    pub beatmaps: Vec<BeatmapOsu>,
    pub title: String,
    pub artist: String,
    pub covers: Cover,
    pub creator: String,
    pub id: u32,
    pub user_id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct BeatmapsetOsu {
    #[serde(flatten)]
    pub base_beatmapset: BaseBeatmapset,
    pub related_users: Vec<BeatmapsetRelatedUser>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuSearchMapResponse {
    pub beatmapsets: Vec<BaseBeatmapset>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuMultipleBeatmap {
    pub id: u32,
    pub difficulty_rating: f32,
    pub mode: String,
    pub beatmapset_id: u32,
    pub version: String,
    pub user_id: u32,
    pub beatmapset: OsuMultipleBeatmapsetResponse,
}

impl GetID for OsuMultipleBeatmap {
    fn get_id(&self) -> u32 {
        self.id
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuMultipleBeatmapsetResponse {
    pub title: String,
    pub artist: String,
    pub covers: Cover,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
pub struct OsuBeatmapSmall {
    pub id: u32,
    pub difficulty_rating: f32,
    pub mode: String,
    pub beatmapset_id: u32,
    pub version: String,
    pub user_id: u32,
    pub user_name: String,
    pub user_avatar_url: String,
    pub title: String,
    pub artist: String,
    pub cover: String,
}

impl OsuBeatmapSmall {
    pub fn from_osu_beatmap_and_user_data(
        osu_multiple: OsuMultipleBeatmap,
        user_name: String,
        user_avatar_url: String,
    ) -> OsuBeatmapSmall {
        OsuBeatmapSmall {
            id: osu_multiple.id,
            difficulty_rating: osu_multiple.difficulty_rating,
            mode: osu_multiple.mode,
            beatmapset_id: osu_multiple.beatmapset_id,
            version: osu_multiple.version,
            user_id: osu_multiple.user_id,
            user_name,
            user_avatar_url,
            title: osu_multiple.beatmapset.title,
            artist: osu_multiple.beatmapset.artist,
            cover: osu_multiple.beatmapset.covers.cover,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, PartialEq)]
#[serde(untagged)]
pub enum BeatmapEnum {
    All(OsuBeatmapSmall),
    Id(u32),
}

impl GetID for BeatmapEnum {
    fn get_id(&self) -> u32 {
        match self {
            BeatmapEnum::All(beatmap) => beatmap.id,
            BeatmapEnum::Id(id) => *id,
        }
    }
}

/// Also has `refresh_token` but we don't need it
#[derive(Serialize, Deserialize, Debug)]
pub struct OsuAuthToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u32,
}

#[derive(Serialize, Debug)]
struct AuthRequest {
    pub client_id: &'static str,
    pub client_secret: &'static str,
    pub grant_type: &'static str,
    pub redirect_uri: &'static str,
    pub scope: Option<&'static str>,
    pub code: Option<String>,
}

impl AuthRequest {
    fn authorization(code: String) -> AuthRequest {
        AuthRequest {
            client_id: &CLIENT_ID,
            client_secret: &CLIENT_SECRET,
            redirect_uri: &REDIRECT_URI,
            grant_type: "authorization_code",
            code: Some(code),
            scope: None,
        }
    }

    fn client_credential() -> AuthRequest {
        AuthRequest {
            client_id: &CLIENT_ID,
            client_secret: &CLIENT_SECRET,
            redirect_uri: &REDIRECT_URI,
            grant_type: "client_credentials",
            code: None,
            scope: Some("public"),
        }
    }
}

pub struct RequestClient {
    client: reqwest::Client,
    semaphore: Semaphore,
}

impl RequestClient {
    pub fn new(concurrent_requests: usize) -> RequestClient {
        RequestClient {
            client: reqwest::Client::new(),
            semaphore: Semaphore::new(concurrent_requests),
        }
    }
    pub async fn get_osu_auth_token(&self, code: String) -> Result<OsuAuthToken, AppError> {
        let token_url = "https://osu.ppy.sh/oauth/token";
        let auth_body = AuthRequest::authorization(code);

        let permit = self.semaphore.acquire().await?;
        let res = self.client.post(token_url).json(&auth_body).send().await?;
        drop(permit);

        let data = res.json::<OsuAuthToken>().await?;
        Ok(data)
    }

    pub async fn get_client_credentials_token(&self) -> Result<OsuAuthToken, AppError> {
        let token_url = "https://osu.ppy.sh/oauth/token";
        let auth_request_body = AuthRequest::client_credential();
        let token: OsuAuthToken = self
            .client
            .post(token_url)
            .json(&auth_request_body)
            .send()
            .await?
            .json()
            .await?;
        Ok(token)
    }

    // Function to make an authenticated GET request and parse the response as T
    async fn request<T: DeserializeOwned>(
        &self,
        url: &str,
        access_token: &str,
    ) -> Result<T, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", access_token).parse().unwrap(),
        );

        let permit = self.semaphore.acquire().await?;
        let res = self.client.get(url).headers(headers).send().await?;
        drop(permit);

        let data = res.json().await?;
        Ok(data)
    }
    pub async fn get_token_user(&self, access_token: &str) -> Result<UserOsu, AppError> {
        let me_url = "https://osu.ppy.sh/api/v2/me";
        self.request(me_url, access_token).await
    }

    pub async fn get_beatmap_osu(
        &self,
        access_token: &str,
        beatmap_id: u32,
    ) -> Result<BeatmapOsu, AppError> {
        let beatmap_url = format!("https://osu.ppy.sh/api/v2/beatmaps/{}", beatmap_id);
        self.request(&beatmap_url, access_token).await
    }

    pub async fn get_beatmapset_osu(
        &self,
        access_token: &str,
        beatmapset_id: u32,
    ) -> Result<BeatmapsetOsu, AppError> {
        let beatmapset_url = format!("https://osu.ppy.sh/api/v2/beatmapsets/{}", beatmapset_id);
        self.request(&beatmapset_url, access_token).await
    }

    pub async fn get_user_osu(
        &self,
        access_token: &str,
        user_id: u32,
    ) -> Result<UserOsu, AppError> {
        let user_url = format!("https://osu.ppy.sh/api/v2/users/{}", user_id);
        self.request(&user_url, access_token).await
    }

    pub async fn search_user_osu(
        &self,
        access_token: &str,
        query: &str,
    ) -> Result<OsuSearchUserResponse, AppError> {
        let search_url = format!(
            "https://osu.ppy.sh/api/v2/search/?mode=user&query={}",
            query
        );
        self.request(&search_url, access_token).await
    }

    pub async fn search_map_osu(
        &self,
        access_token: &str,
        query: &str,
    ) -> Result<OsuSearchMapResponse, AppError> {
        let search_url = format!("https://osu.ppy.sh/api/v2/beatmapsets/search?{}", query);
        self.request(&search_url, access_token).await
    }

    async fn request_and_deserialize_without_outer_layer<T: DeserializeOwned>(
        &self,
        url: String,
        access_token: String,
    ) -> Result<Vec<T>, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", access_token).parse().unwrap(),
        );

        let permit = self.semaphore.acquire().await?;
        let res = self.client.get(url).headers(headers).send().await?;
        drop(permit);

        let text = &res.text().await?;
        let data: Value = serde_json::from_str(text)?;
        let inner = data
            .as_object()
            .ok_or(AppError::MissingLayerJson)?
            .values()
            .nth(0)
            .ok_or(AppError::MissingLayerJson)?;

        let with_type: Vec<T> = serde_json::from_value(inner.clone())?;
        Ok(with_type)
    }

    pub async fn request_multiple<T: DeserializeOwned + std::marker::Send + 'static>(
        self: Arc<Self>,
        base_url: &str,
        keys: &[u32],
        access_token: &str,
    ) -> Result<Vec<T>, AppError> {
        let mut handlers = Vec::new();
        for chunk_ids in keys.chunks(50) {
            let url = format!(
                "{}?{}",
                base_url,
                chunk_ids
                    .iter()
                    .map(|id| format!("ids[]={}", id))
                    .collect::<Vec<_>>()
                    .join("&")
            );
            let access_token_string = access_token.to_string();
            let self_clone = Arc::clone(&self);

            let handler = tokio::spawn(async move {
                let response: Result<Vec<T>, AppError> = self_clone
                    .request_and_deserialize_without_outer_layer(url, access_token_string)
                    .await;
                response
            });
            handlers.push(handler);
        }

        try_join_all(handlers)
            .await?
            .into_iter()
            .try_fold(vec![], |mut acc, result| {
                acc.extend(result?);
                Ok(acc)
            })
    }
}

impl Retryable for Arc<RequestClient> {
    type Value = OsuAuthToken;
    type Err = AppError;
    async fn retry(&mut self) -> Result<OsuAuthToken, RetryAction<AppError>> {
        self.get_client_credentials_token().await.map_err(|err| {
            RetryAction::new(
                err,
                "Failed to get client credential grant token".to_string(),
                RetryOption::Retry,
            )
        })
    }
}

/// A wrapper to [`RequestClient`] to store and update credentials grant client auth method token
///
/// Will be used to request activity data and daily update data
pub struct CredentialsGrantClient {
    client: Arc<RequestClient>,
    token: RwLock<String>,
}

impl CredentialsGrantClient {
    pub async fn new(client: Arc<RequestClient>) -> Result<Arc<CredentialsGrantClient>, AppError> {
        let client = Arc::new(CredentialsGrantClient {
            client,
            token: RwLock::new(String::new()),
        });
        client.clone().start_loop();
        Ok(client)
    }

    fn update_token(&self, new_token: String) -> Result<(), AppError> {
        let mut token = self.token.write().map_err(|_| AppError::RwLock)?;
        *token = new_token;
        Ok(())
    }

    fn start_loop(self: Arc<Self>) {
        let buffer_time = 120;
        let client_clone = self.client.clone();
        let mut retryer = Retry::new(60, client_clone);

        tokio::spawn(async move {
            loop {
                // we can't fail this task, best we can do is to retry. If this doesn't work,
                // then there is a good chance that the rest of the requests won't work either
                let token = retryer.retry_until_success().await;

                let _ = self.update_token(token.access_token);
                sleep(Duration::from_secs(token.expires_in as u64 - buffer_time)).await;
            }
        });
    }

    pub fn get_access_token(&self) -> Result<String, AppError> {
        let token = self.token.read().map_err(|_| AppError::RwLock)?;
        Ok(token.clone())
    }

    /// Ease of use to get user data since we already contain the client inside
    pub async fn get_user_osu(self, user_id: u32) -> Result<UserOsu, AppError> {
        let token = self.get_access_token()?;
        self.client.get_user_osu(&token, user_id).await
    }
}

pub struct CachedRequester<T: DeserializeOwned + GetID + Clone + Send + 'static> {
    pub client: Arc<RequestClient>,
    pub cache: Mutex<CustomCache<u32, T>>,
    pub base_url: String,
}

impl<T: DeserializeOwned + GetID + Clone + Send + 'static> CachedRequester<T> {
    pub fn new(
        client: Arc<RequestClient>,
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
        let misses_requested: Vec<T> = self
            .client
            .clone()
            .request_multiple(&self.base_url, &cache_result.misses, access_token)
            .await?;

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
    pub fn new(client: Arc<RequestClient>) -> Arc<Self> {
        let user_requester = Arc::new(CachedRequester::new(
            client.clone(),
            "https://osu.ppy.sh/api/v2/users",
            24600,
        ));
        let beatmap_requester = Arc::new(CachedRequester::new(
            client.clone(),
            "https://osu.ppy.sh/api/v2/beatmaps",
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
            .collect();
        let user_map = self
            .user_requester
            .clone()
            .get_multiple_osu(&users_to_request, access_token)
            .await?;
        let combined = beatmap_map
            .into_iter()
            .filter_map(|(beatmap_id, beatmap)| {
                let user = user_map.get(&beatmap.user_id)?;
                let new_beatmap = OsuBeatmapSmall::from_osu_beatmap_and_user_data(
                    beatmap,
                    user.username.clone(),
                    user.avatar_url.clone(),
                );
                Some((beatmap_id, new_beatmap))
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
}

#[cached(
    ty = "CustomCache<u32, UserOsu>",
    create = "{CustomCache::new(21600)}",
    convert = r#"{user_id}"#,
    result = true
)]
pub async fn cached_osu_user_request(
    client: Arc<RequestClient>,
    osu_token: &str,
    user_id: u32,
) -> Result<UserOsu, AppError> {
    let user_osu = client.get_user_osu(osu_token, user_id).await?;
    Ok(user_osu)
}
