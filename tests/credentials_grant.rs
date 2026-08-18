use std::sync::Arc;

use common::osu_test_client::OsuApiTestClient;
use futures::future::join_all;
use mapper_influences_backend_rs::osu_api::{
    credentials_grant::CredentialsGrantClient, request::OsuApiRequestClient,
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
