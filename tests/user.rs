use common::init_test_env;
use http::header::COOKIE;
use mapper_influences_backend_rs::{
    database::user::User,
    handlers::{auth::AdminLogin, BeatmapRequest},
};

mod common;

#[tokio::test]
async fn test_user_beatmap_add() {
    const TEST_LABEL: &str = "UserBeatmapAdd";
    let test_env = init_test_env(TEST_LABEL).await;

    let oauth_body = AdminLogin::new(std::env::var("ADMIN_PASSWORD").unwrap(), 2);
    let jwt = test_env
        .server
        .post("/oauth/admin")
        .json(&oauth_body)
        .await
        .text();

    let _result: User = test_env
        .server
        .patch("/users/map")
        .add_header(COOKIE, format!("user_token={}", jwt))
        .json(&BeatmapRequest {
            ids: vec![4823239, 3119298, 3119298].into_iter().collect(),
        })
        .await
        .json();

    test_env
        .requester
        .save_cache()
        .expect("failed to save cache");
}
