use common::init_test_env;

mod common;

#[tokio::test]
async fn test_beatmap_leaderboard() {
    const TEST_LABEL: &str = "BeatmapLeaderboard";
    let test_env = init_test_env(TEST_LABEL).await;
    let _response = test_env.server.get("/leaderboard/beatmap").await;
    test_env
        .requester
        .save_cache()
        .expect("failed to save cache");
}
