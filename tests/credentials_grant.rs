use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;

use axum::async_trait;
use bytes::Bytes;
use common::osu_test_client::OsuApiTestClient;
use futures::future::join_all;
use mapper_influences_backend_rs::{
    error::AppError,
    osu_api::{
        credentials_grant::CredentialsGrantClient,
        request::{OsuApiRequestClient, Requester},
        AuthRequest, OsuAuthToken,
    },
};

mod common;

/// The token is fetched lazily, so everything that starts up at once (the cache
/// refresh routine and the first requests) races for it
#[tokio::test]
async fn test_concurrent_first_token_access() {
    // Replaying, so no osu! API call is made for the token
    const TEST_LABEL: &str = "BeatmapLeaderboard";
    let working_request_client = Arc::new(OsuApiRequestClient::new(10));
    let test_request_client = OsuApiTestClient::new(working_request_client, TEST_LABEL);
    let credentials_grant_client = CredentialsGrantClient::new(test_request_client)
        .await
        .expect("Failed to initialize credentials grant client");

    let handles: Vec<_> = (0..16)
        .map(|_| {
            let client = credentials_grant_client.clone();
            tokio::spawn(async move { client.get_access_token().await })
        })
        .collect();

    for result in join_all(handles).await {
        result
            .expect("a token access panicked")
            .expect("failed to get the access token");
    }
}

/// Lifetime reported for every handed out token. Long enough to clear `MIN_REFRESH_SECS`, so the
/// refresh lands at `TOKEN_LIFETIME_SECS` minus the refresh buffer, 80 seconds in.
const TOKEN_LIFETIME_SECS: u64 = 200;

/// Hands out a new token on every call, so a token tells us which refresh produced it.
struct CountingTokenClient {
    tokens_issued: AtomicU32,
}

#[async_trait]
impl Requester for CountingTokenClient {
    async fn get_request(&self, _url: &str, _token: &str) -> Result<Bytes, AppError> {
        unreachable!("this test only exercises the token refresh loop")
    }
    async fn post_request(&self, _url: &str, _body: AuthRequest) -> Result<Bytes, AppError> {
        unreachable!("this test only exercises the token refresh loop")
    }
    async fn get_client_credentials_token(&self) -> Result<OsuAuthToken, AppError> {
        let issued = self.tokens_issued.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(OsuAuthToken {
            access_token: format!("token-{}", issued),
            expires_in: TOKEN_LIFETIME_SECS as u32,
        })
    }
}

/// The refresh loop publishes over a watch channel, and a watch send fails once every receiver is
/// gone. Callers only subscribe for the length of a single `get_access_token` call, so the loop has
/// to keep publishing on its own, long after the first caller has come and gone. When it doesn't,
/// the expired first token stays readable and every osu! request 401s until a restart.
#[tokio::test(start_paused = true)]
async fn test_token_keeps_refreshing_after_the_first_caller_leaves() {
    let request_client = Arc::new(CountingTokenClient {
        tokens_issued: AtomicU32::new(0),
    });
    let credentials_grant_client = CredentialsGrantClient::new(request_client)
        .await
        .expect("Failed to initialize credentials grant client");

    let first = credentials_grant_client
        .get_access_token()
        .await
        .expect("failed to get the first access token");
    assert_eq!(first, "token-1");

    // Past the refresh, but short of a second one. The clock is paused, so this hands control to
    // the refresh loop rather than actually waiting.
    tokio::time::sleep(Duration::from_secs(TOKEN_LIFETIME_SECS / 2)).await;

    let refreshed = credentials_grant_client
        .get_access_token()
        .await
        .expect("failed to get the refreshed access token");
    assert_eq!(
        refreshed, "token-2",
        "the refresh loop stopped after the first token, leaving an expired one in place"
    );
}
