use std::{sync::Arc, time::Duration};

use axum::async_trait;
use bytes::Bytes;
use common::{init_test_env, TestEnv};
use mapper_influences_backend_rs::{
    daily_update::update_once,
    database::numerical_thing,
    error::AppError,
    osu_api::{
        credentials_grant::CredentialsGrantClient, request::Requester, AuthRequest, OsuAuthToken,
    },
};

mod common;

/// The user `init_test_env` seeds the database with.
const SEEDED_USER_ID: u32 = 2;

/// Replaying the requests of a cache that already exists, so no osu! API call is made
const TEST_LABEL: &str = "BeatmapLeaderboard";

/// Fails every user request with a fixed status. Stands in for a restricted user (404) or for the
/// osu! API being unhappy with us for reasons that pass (503).
struct FailingUserClient {
    status: u16,
}

#[async_trait]
impl Requester for FailingUserClient {
    async fn get_request(&self, _url: &str, _token: &str) -> Result<Bytes, AppError> {
        Err(AppError::OsuApiStatus(self.status))
    }
    async fn post_request(&self, _url: &str, _body: AuthRequest) -> Result<Bytes, AppError> {
        unreachable!("the token is handed out without a request")
    }
    async fn get_client_credentials_token(&self) -> Result<OsuAuthToken, AppError> {
        Ok(OsuAuthToken::test())
    }
}

async fn failing_client(status: u16) -> Arc<CredentialsGrantClient> {
    CredentialsGrantClient::new(Arc::new(FailingUserClient { status }))
        .await
        .expect("Failed to initialize credentials grant client")
}

/// Moves a user's update date past the window `get_users_to_update` selects on, which is what a
/// user who hasn't been picked up in over a week looks like.
///
/// `updated_at` is defined with a `VALUE time::now()` clause, so it rewrites itself on every write
/// and a plain update can't plant an old date. The clause is dropped for that one write and then
/// put back exactly as `migrations/schemas/user.surql` defines it.
async fn move_user_into_the_update_window(test_env: &TestEnv, user_id: u32) {
    test_env
        .state
        .db
        .get_inner_ref()
        .query("DEFINE FIELD OVERWRITE updated_at ON user TYPE datetime")
        .query("UPDATE $thing SET updated_at = time::now() - 2w")
        .bind(("thing", numerical_thing("user", user_id)))
        .query("DEFINE FIELD OVERWRITE updated_at ON user TYPE datetime VALUE time::now()")
        .await
        .expect("failed to move the user into the update window")
        .check()
        .expect("failed to move the user into the update window");
}

/// Restricted users are still ours, but the osu! API stops handing them out. Their update date has
/// to move anyway, otherwise they come back in every single daily run and fail every time.
#[tokio::test]
async fn test_restricted_user_leaves_the_daily_update_window() {
    let test_env = init_test_env(TEST_LABEL).await;
    move_user_into_the_update_window(&test_env, SEEDED_USER_ID).await;

    let users_to_update = test_env.state.db.get_users_to_update().await.unwrap();
    assert_eq!(
        users_to_update,
        vec![SEEDED_USER_ID],
        "the user should be due for an update before the run"
    );

    let unsuccessfull_ids = update_once(
        failing_client(404).await,
        test_env.state.db.clone(),
        users_to_update,
        Duration::from_millis(1),
    )
    .await;

    assert!(
        unsuccessfull_ids.is_empty(),
        "a restricted user is handled, not worth retrying: {:?}",
        unsuccessfull_ids
    );
    assert!(
        test_env
            .state
            .db
            .get_users_to_update()
            .await
            .unwrap()
            .is_empty(),
        "a restricted user should be out of the window until it comes back around"
    );
}

/// The counterpart: a failure that says nothing about the user must leave them where they are, so
/// the next run tries again. A bad token 401s every user in the run, and skipping them all on that
/// basis would push the entire database out of the window on a single bad afternoon.
#[tokio::test]
async fn test_transient_failure_keeps_the_user_in_the_daily_update_window() {
    let test_env = init_test_env(TEST_LABEL).await;
    move_user_into_the_update_window(&test_env, SEEDED_USER_ID).await;

    let users_to_update = test_env.state.db.get_users_to_update().await.unwrap();

    let unsuccessfull_ids = update_once(
        failing_client(503).await,
        test_env.state.db.clone(),
        users_to_update,
        Duration::from_millis(1),
    )
    .await;

    assert_eq!(
        unsuccessfull_ids,
        vec![SEEDED_USER_ID],
        "a transient failure should be reported as unsuccessful"
    );
    assert_eq!(
        test_env.state.db.get_users_to_update().await.unwrap(),
        vec![SEEDED_USER_ID],
        "a transient failure should leave the user due for the next run"
    );
}
