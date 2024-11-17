use common::init_test_env;

mod common;

#[tokio::test]
async fn test_beatmap_leaderboard() {
    const TEST_LABEL: &str = "BeatmapLeaderboard";
    let (test_server, test_requester) = init_test_env(TEST_LABEL).await;
    let _response = test_server.get("/leaderboard/beatmap").await;
    test_requester.save_cache().expect("failed to save cache");
}
