use std::time::Duration;

use common::init_test_env;
use mapper_influences_backend_rs::{
    cache_update::{
        refresh_beatmap_leaderboard, refresh_graph, refresh_user_leaderboard, spawn_cache_updaters,
    },
    handlers::leaderboard::user_leaderboard_key,
};

mod common;

/// The caches have to be filled by the background refresh alone, without any
/// request hitting the endpoints first
#[tokio::test]
async fn test_refresh_fills_the_caches() {
    // Reusing a recording, this test doesn't reach the osu! API on its own
    const TEST_LABEL: &str = "BeatmapLeaderboard";
    let test_env = init_test_env(TEST_LABEL).await;
    let state = &test_env.state;

    let global_key = user_leaderboard_key(false, None);
    assert!(
        state
            .user_leaderboard_cache
            .get_leaderboard(&global_key)
            .await
            .is_none(),
        "the user leaderboard cache should start out empty"
    );
    assert!(
        state.graph_cache.get_data().await.is_none(),
        "the graph cache should start out empty"
    );

    // Each of these is driven by its own updater in production
    refresh_user_leaderboard(state, false).await.unwrap();
    refresh_beatmap_leaderboard(state, false).await.unwrap();
    refresh_graph(state).await.unwrap();

    assert!(
        state
            .user_leaderboard_cache
            .get_leaderboard(&global_key)
            .await
            .is_some(),
        "the user leaderboard should be cached after a refresh"
    );
    assert!(
        state
            .beatmap_leaderboard_cache
            .get_leaderboard("false")
            .await
            .is_some(),
        "the beatmap leaderboard should be cached after a refresh"
    );
    assert!(
        state.graph_cache.get_data().await.is_some(),
        "the graph data should be cached after a refresh"
    );
}

/// Every updater is its own task and runs a first cycle as soon as its start delay
/// is over, so spawning them is enough to fill the caches
#[tokio::test]
async fn test_updaters_fill_the_caches_on_their_own() {
    const TEST_LABEL: &str = "BeatmapLeaderboard";
    let test_env = init_test_env(TEST_LABEL).await;
    let state = &test_env.state;
    let global_key = user_leaderboard_key(false, None);

    // Same staggered startup as production, just not in 20 second steps
    spawn_cache_updaters(state.clone(), Duration::from_millis(50));

    let filled = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let user_leaderboard = state
                .user_leaderboard_cache
                .get_leaderboard(&global_key)
                .await
                .is_some();
            let beatmap_leaderboard = state
                .beatmap_leaderboard_cache
                .get_leaderboard("false")
                .await
                .is_some();
            let graph = state.graph_cache.get_data().await.is_some();

            if user_leaderboard && beatmap_leaderboard && graph {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    assert!(
        filled.is_ok(),
        "the spawned updaters should fill the caches without any request coming in"
    );
}
