use common::init_test_env;
use http::header::COOKIE;
use mapper_influences_backend_rs::{database::user::User, handlers::auth::AdminLogin};

mod common;

#[tokio::test]
async fn test_user_beatmap_add() {
    const TEST_LABEL: &str = "UserBeatmapAdd";
    let (test_server, test_requester, _testcontainer_handle) = init_test_env(TEST_LABEL).await;

    let oauth_body = AdminLogin::new(std::env::var("ADMIN_PASSWORD").unwrap(), 2);
    let jwt = test_server
        .post("/oauth/admin")
        .json(&oauth_body)
        .await
        .text();

    let _result: User = test_server
        .patch("/users/map/4776938")
        .add_header(COOKIE, format!("user_token={}", jwt))
        .await
        .json();

    test_requester.save_cache().expect("failed to save cache");
}
