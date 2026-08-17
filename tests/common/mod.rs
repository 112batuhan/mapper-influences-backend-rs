// Every test binary compiles this module on its own, so whatever a single one of
// them doesn't touch looks dead from its point of view
#![allow(dead_code)]

use std::sync::Arc;

use axum::Router;
use axum_test::TestServer;
use mapper_influences_backend_rs::{
    cache::RedisCache,
    database::DatabaseClient,
    osu_api::{credentials_grant::CredentialsGrantClient, request::OsuApiRequestClient},
    routes, AppState,
};
use osu_test_client::OsuApiTestClient;
use surrealdb_migrations::MigrationRunner;
use testcontainers_modules::{
    redis::{Redis, REDIS_PORT},
    surrealdb::{SurrealDb, SURREALDB_PORT},
    testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt},
};
use tokio::sync::Mutex;

pub mod osu_test_client;

/// See where it's used, migrations are not safe to run side by side
static MIGRATION_LOCK: Mutex<()> = Mutex::const_new(());

/// Containers are dropped when this is dropped, so tests have to keep it alive
pub struct TestContainers {
    pub _surrealdb: ContainerAsync<SurrealDb>,
    pub _redis: ContainerAsync<Redis>,
}

pub struct TestEnv {
    pub server: TestServer,
    pub requester: Arc<OsuApiTestClient>,
    /// For the tests that need to reach past the endpoints, like the background routines
    pub state: Arc<AppState>,
    pub _containers: TestContainers,
}

pub async fn init_test_env(label: &str) -> TestEnv {
    dotenvy::dotenv().ok();

    // Think of this as join handler. we need to keep the reference alive.
    // Db closes when we drop this. Luckly it's enough to return this and forget.
    let surrealdb_container = SurrealDb::default()
        .with_authentication(false)
        .with_user("backend")
        .with_password("password")
        .with_tag("v2.1.0")
        .start()
        .await
        .unwrap();

    let host_port = surrealdb_container
        .get_host_port_ipv4(SURREALDB_PORT)
        .await
        .expect("Failed to start SurrealDB test container");
    let url = format!("ws://127.0.0.1:{host_port}");
    let db = DatabaseClient::new(&url)
        .await
        .expect("failed to initialize db connection");

    {
        // Applying migrations rewrites `migrations/migrations/definitions/_initial.json`,
        // and a concurrent runner reads that file while it's still empty. The databases
        // are separate, the file isn't, so only one test at a time gets to migrate.
        let _migration_guard = MIGRATION_LOCK.lock().await;
        MigrationRunner::new(db.get_inner_ref())
            .up()
            .await
            .expect("Failed to apply migrations");
    }

    // Every test gets its own redis instance, so caches don't leak between tests.
    // The module defaults to redis 5.0, so the tag is pinned to the one in docker compose
    let redis_container = Redis::default()
        .with_tag("7-alpine")
        .start()
        .await
        .expect("Failed to start redis test container");
    let redis_host_port = redis_container
        .get_host_port_ipv4(REDIS_PORT)
        .await
        .expect("Failed to get the redis container port");
    let cache = RedisCache::new(&format!("redis://127.0.0.1:{redis_host_port}"))
        .await
        .expect("failed to initialize redis connection");

    let working_request_client = Arc::new(OsuApiRequestClient::new(10));
    let test_request_client = OsuApiTestClient::new(working_request_client.clone(), label);
    let credentials_grant_client = CredentialsGrantClient::new(test_request_client.clone())
        .await
        .expect("Failed to initialize credentials grant client");

    let state = AppState::new(
        test_request_client.clone(),
        credentials_grant_client,
        db,
        cache,
    )
    .await;

    // Requesting peppy to add in our initial database
    let test_initial_user = state
        .credentials_grant_client
        .get_user_osu(2)
        .await
        .unwrap();
    state.db.upsert_user(test_initial_user).await.unwrap();

    // The exact routes and middleware the binary serves, without the openapi
    // documentation. `ApiRouter` can't be handed to axum_test directly, but it
    // converts into a plain `Router`
    let routes = Router::from(routes(state.clone())).with_state(state.clone());
    let test_server = TestServer::new(routes).expect("failed to initialize test server");
    TestEnv {
        server: test_server,
        requester: test_request_client,
        state,
        _containers: TestContainers {
            _surrealdb: surrealdb_container,
            _redis: redis_container,
        },
    }
}
