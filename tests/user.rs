use common::init_test_env;
use http::header::COOKIE;
use mapper_influences_backend_rs::handlers::auth::AdminLogin;

mod common;

#[tokio::test]
async fn test_beatmap_leaderboard() {
    const TEST_LABEL: &str = "UserBeatmapAdd";
    let (test_server, test_requester) = init_test_env(TEST_LABEL).await;

    let oauth_body = AdminLogin::new(std::env::var("ADMIN_PASSWORD").unwrap(), 2);
    let jwt = test_server
        .post("/oauth/admin")
        .json(&oauth_body)
        .await
        .text();

    let result = test_server
        .patch("/users/map/4776938")
        .add_header(COOKIE, format!("user_token={}", jwt))
        .await
        .text();

    dbg!(result);
    test_requester.save_cache().expect("failed to save cache");
}
