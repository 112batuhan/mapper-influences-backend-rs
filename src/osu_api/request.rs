use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::future::try_join_all;
use http::{header::AUTHORIZATION, HeaderMap};
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::{error::AppError, retry::Retryable};

use super::{
    AuthRequest, BeatmapOsu, BeatmapsetOsu, OsuAuthToken, OsuSearchMapResponse,
    OsuSearchUserResponse, UserOsu,
};

/// The reason that the requests retun bytes and then they get decoded, is that it's exaclty the
/// same implementation in `res.json().await`. this allows us to deserialize bodies into any
/// type we want in spesific implementation while keeping the return types non generic.
#[async_trait]
pub trait Requester
where
    Self: Send + Sync + 'static,
{
    async fn get_request(&self, url: &str, token: &str) -> Result<Bytes, AppError>;
    async fn post_request(&self, url: &str, body: AuthRequest) -> Result<Bytes, AppError>;
    async fn get_osu_auth_token(&self, code: String) -> Result<OsuAuthToken, AppError> {
        let token_url = "https://osu.ppy.sh/oauth/token";
        let auth_body = AuthRequest::authorization(code);
        let res_body_bytes = self.post_request(token_url, auth_body).await?;
        Ok(serde_json::from_slice(&res_body_bytes)?)
    }
    async fn get_client_credentials_token(&self) -> Result<OsuAuthToken, AppError> {
        let token_url = "https://osu.ppy.sh/oauth/token";
        let auth_body = AuthRequest::client_credential();
        let res_body_bytes = self.post_request(token_url, auth_body).await?;
        Ok(serde_json::from_slice(&res_body_bytes)?)
    }
    async fn get_token_user(&self, access_token: &str) -> Result<UserOsu, AppError> {
        let me_url = "https://osu.ppy.sh/api/v2/me";
        let res_body_bytes = self.get_request(me_url, access_token).await?;
        Ok(serde_json::from_slice(&res_body_bytes)?)
    }
    async fn get_beatmap_osu(
        &self,
        access_token: &str,
        beatmap_id: u32,
    ) -> Result<BeatmapOsu, AppError> {
        let beatmap_url = format!("https://osu.ppy.sh/api/v2/beatmaps/{}", beatmap_id);
        let res_body_bytes = self.get_request(&beatmap_url, access_token).await?;
        Ok(serde_json::from_slice(&res_body_bytes)?)
    }

    async fn get_beatmapset_osu(
        &self,
        access_token: &str,
        beatmapset_id: u32,
    ) -> Result<BeatmapsetOsu, AppError> {
        let beatmapset_url = format!("https://osu.ppy.sh/api/v2/beatmapsets/{}", beatmapset_id);
        let res_body_bytes = self.get_request(&beatmapset_url, access_token).await?;
        Ok(serde_json::from_slice(&res_body_bytes)?)
    }

    async fn get_user_osu(&self, access_token: &str, user_id: u32) -> Result<UserOsu, AppError> {
        let user_url = format!("https://osu.ppy.sh/api/v2/users/{}", user_id);
        let res_body_bytes = self.get_request(&user_url, access_token).await?;
        Ok(serde_json::from_slice(&res_body_bytes)?)
    }

    async fn search_user_osu(
        &self,
        access_token: &str,
        query: &str,
    ) -> Result<OsuSearchUserResponse, AppError> {
        let search_url = format!(
            "https://osu.ppy.sh/api/v2/search/?mode=user&query={}",
            query
        );
        let res_body_bytes = self.get_request(&search_url, access_token).await?;
        Ok(serde_json::from_slice(&res_body_bytes)?)
    }

    async fn search_map_osu(
        &self,
        access_token: &str,
        query: &str,
    ) -> Result<OsuSearchMapResponse, AppError> {
        let search_url = format!("https://osu.ppy.sh/api/v2/beatmapsets/search?{}", query);
        let res_body_bytes = self.get_request(&search_url, access_token).await?;
        Ok(serde_json::from_slice(&res_body_bytes)?)
    }

    async fn deserialize_without_outer(
        &self,
        url: String,
        access_token: String,
    ) -> Result<Vec<Value>, AppError> {
        let res_body_bytes = self.get_request(&url, &access_token).await?;
        let data: Value = serde_json::from_slice(&res_body_bytes)?;

        let inner = data
            .as_object()
            .ok_or(AppError::MissingLayerJson)?
            .values()
            .nth(0)
            .ok_or(AppError::MissingLayerJson)?
            .as_array()
            .ok_or(AppError::MissingLayerJson)?;
        Ok(inner.clone())
    }

    async fn request_multiple(
        self: Arc<Self>,
        base_url: &str,
        keys: &[u32],
        access_token: &str,
    ) -> Result<Vec<Value>, AppError> {
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
                let response: Result<Vec<Value>, AppError> = self_clone
                    .deserialize_without_outer(url, access_token_string)
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

pub struct OsuApiRequestClient {
    client: reqwest::Client,
    semaphore: Semaphore,
}
impl OsuApiRequestClient {
    pub fn new(concurrent_requests: usize) -> OsuApiRequestClient {
        OsuApiRequestClient {
            client: reqwest::Client::new(),
            semaphore: Semaphore::new(concurrent_requests),
        }
    }
}

#[async_trait]
impl Requester for OsuApiRequestClient {
    async fn get_request(&self, url: &str, access_token: &str) -> Result<Bytes, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", access_token).parse().unwrap(),
        );

        let _permit = self.semaphore.acquire().await?;
        let res = self.client.get(url).headers(headers).send().await?;
        Ok(res.bytes().await?)
    }

    async fn post_request(&self, url: &str, body: AuthRequest) -> Result<Bytes, AppError> {
        let _permit = self.semaphore.acquire().await?;
        let res = self.client.post(url).json(&body).send().await?;
        Ok(res.bytes().await?)
    }
}

impl Retryable for Arc<dyn Requester> {
    type Value = OsuAuthToken;
    type Err = AppError;
    async fn retry(&mut self) -> Result<OsuAuthToken, AppError> {
        self.get_client_credentials_token().await
    }
}
