use std::time::{Duration, Instant};

use common::init_test_env;

mod common;

/// The deployment once went down because every cache error turned into a 500, and
/// the endpoints have to keep answering from the database when redis is gone.
/// They also have to answer quickly, waiting out a dead cache on every request is
/// its own kind of outage.
#[tokio::test]
async fn test_endpoints_survive_redis_going_away() {
    const TEST_LABEL: &str = "BeatmapLeaderboard";
    let test_env = init_test_env(TEST_LABEL).await;

    let healthy = test_env.server.get("/graph").await;
    assert_eq!(
        healthy.status_code(),
        200,
        "the graph endpoint should work with redis up"
    );

    test_env
        ._containers
        ._redis
        .stop()
        .await
        .expect("failed to stop the redis container");

    let started = Instant::now();
    let graph = test_env.server.get("/graph").await;
    let graph_elapsed = started.elapsed();

    let started = Instant::now();
    let leaderboard = test_env.server.get("/leaderboard/user").await;
    let leaderboard_elapsed = started.elapsed();

    assert_eq!(
        graph.status_code(),
        200,
        "the graph endpoint should fall back to the database with redis down"
    );
    assert_eq!(
        leaderboard.status_code(),
        200,
        "the leaderboard endpoint should fall back to the database with redis down"
    );

    // Generous next to the sub second responses this actually gives, it's here to
    // catch a client that goes back to retrying its way through every request
    let ceiling = Duration::from_secs(5);
    assert!(
        graph_elapsed < ceiling && leaderboard_elapsed < ceiling,
        "requests should not wait on a dead cache, took {graph_elapsed:?} and {leaderboard_elapsed:?}"
    );
}
