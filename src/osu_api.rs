use std::{
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::error::AppError;

static CLIENT_ID: LazyLock<String> =
    LazyLock::new(|| std::env::var("CLIENT_ID").expect("Missing CLIENT_ID environment variable"));

static CLIENT_SECRET: LazyLock<String> = LazyLock::new(|| {
    std::env::var("CLIENT_SECRET").expect("Missing CLIENT_SECRET environment variable")
});

static REDIRECT_URI: LazyLock<String> = LazyLock::new(|| {
    std::env::var("REDIRECT_URI").expect("Missing REDIRECT_URI environment variable")
});

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserId {
    pub id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Country {
    pub code: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Group {
    pub colour: Option<String>,
    pub name: String,
    pub short_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OsuSearchUserData {
    pub data: Vec<UserId>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OsuSearchUserResponse {
    pub user: OsuSearchUserData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BeatmapOsu {
    pub difficulty_rating: f64,
    pub id: u32,
    pub mode: String,
    pub beatmapset_id: u32,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BeatmapsetRelatedUser {
    pub username: String,
    pub avatar_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Cover {
    pub cover: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BaseBeatmapset {
    pub beatmaps: Vec<BeatmapOsu>,
    pub title: String,
    pub artist: String,
    pub covers: Cover,
    pub creator: String,
    pub id: u32,
    pub user_id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BeatmapsetOsu {
    #[serde(flatten)]
    pub base_beatmapset: BaseBeatmapset,
    pub related_users: Vec<BeatmapsetRelatedUser>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OsuSearchMapResponse {
    pub beatmapsets: Vec<BaseBeatmapset>,
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
    semaphore: Arc<Semaphore>,
}

impl RequestClient {
    pub fn new(concurrent_requests: usize) -> RequestClient {
        RequestClient {
            client: reqwest::Client::new(),
            semaphore: Arc::new(Semaphore::new(concurrent_requests)),
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
    async fn request<T: serde::de::DeserializeOwned>(
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

        let data = res.json::<T>().await?;
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
}

/// A wrapper around [`RequestClient`] to make calls using Client Credentials Grant auth method
pub struct CredentialsGrantClient {
    client: RequestClient,
    access_token: String,
    expires_in: Duration,
    auth_time: Instant,
}

impl CredentialsGrantClient {
    pub async fn new() -> Result<CredentialsGrantClient, AppError> {
        // random big number for sephemore permits. We won't ever reach that number using this
        let client = RequestClient::new(500);
        let token = client.get_client_credentials_token().await?;

        Ok(CredentialsGrantClient {
            client,
            access_token: token.access_token,
            expires_in: Duration::from_secs(token.expires_in.into()),
            auth_time: Instant::now(),
        })
    }

    async fn check_token_expiration_and_update(&mut self) -> Result<(), AppError> {
        if self.auth_time.elapsed() > self.expires_in {
            let token = self.client.get_client_credentials_token().await?;
            self.access_token = token.access_token;
            self.auth_time = Instant::now();
            self.expires_in = Duration::from_secs(token.expires_in.into())
        }
        Ok(())
    }

    pub async fn get_user_osu(&mut self, user_id: u32) -> Result<UserOsu, AppError> {
        self.check_token_expiration_and_update().await?;
        self.client.get_user_osu(&self.access_token, user_id).await
    }
}
