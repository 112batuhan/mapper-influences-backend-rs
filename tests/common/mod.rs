use std::sync::Arc;

use axum::{
    middleware,
    routing::{any, delete, get, patch, post},
    Router,
};
use axum_test::TestServer;
use mapper_influences_backend_rs::{
    database::DatabaseClient,
    handlers,
    osu_api::request::{OsuApiRequestClient, Requester},
    AppState,
};
use osu_test_client::OsuApiTestClient;
use surrealdb_migrations::MigrationRunner;
use testcontainers_modules::{
    surrealdb::{SurrealDb, SURREALDB_PORT},
    testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt},
};

pub mod osu_test_client;

/// Redefining routes because aide and axum_test is not compatible
pub fn test_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/search/map", get(handlers::osu_search::osu_beatmap_search))
        .route(
            "/search/user/:query",
            get(handlers::osu_search::osu_user_search),
        )
        .route(
            "/influence/:influenced_to",
            post(handlers::influence::add_influence),
        )
        .route(
            "/influence/influences/:user_id",
            get(handlers::influence::get_user_influences),
        )
        .route(
            "/influence/mentions/:user_id",
            get(handlers::influence::get_user_mentions),
        )
        .route(
            "/influence/:influenced_to",
            delete(handlers::influence::delete_influence),
        )
        .route(
            "/influence/:influenced_to/map/:beatmap_id",
            patch(handlers::influence::add_influence_beatmap),
        )
        .route(
            "/influence/:influenced_to/map/:beatmap_id",
            delete(handlers::influence::remove_influence_beatmap),
        )
        .route(
            "/influence/:influenced_to/description",
            patch(handlers::influence::update_influence_description),
        )
        .route(
            "/influence/:influenced_to/type/:type_id",
            patch(handlers::influence::update_influence_type),
        )
        .route("/users/me", get(handlers::user::get_me))
        .route("/users/:user_id", get(handlers::user::get_user))
        .route("/users/bio", patch(handlers::user::update_user_bio))
        .route(
            "/users/map/:beatmap_id",
            patch(handlers::user::add_user_beatmap),
        )
        .route(
            "/users/map/:beatmap_id",
            delete(handlers::user::delete_user_beatmap),
        )
        .route(
            "/users/influence-order",
            post(handlers::user::set_influence_order),
        )
        .layer(middleware::from_fn_with_state(
            state,
            handlers::auth::check_jwt_token,
        ))
        .route("/activity", get(handlers::activity::get_latest_activities))
        .route("/ws", any(handlers::activity::ws_handler))
        .route(
            "/oauth/osu-redirect",
            get(handlers::auth::osu_oauth2_redirect),
        )
        .route("/oauth/logout", get(handlers::auth::logout))
        .route("/oauth/admin", post(handlers::auth::admin_login))
        .route(
            "/leaderboard/user",
            get(handlers::leaderboard::get_user_leaderboard),
        )
        .route(
            "/leaderboard/beatmap",
            get(handlers::leaderboard::get_beatmap_leaderboard),
        )
        .route("/graph", get(handlers::graph_vizualizer::get_graph_data))
}

pub async fn init_test_env(
    label: &str,
) -> (TestServer, Arc<OsuApiTestClient>, ContainerAsync<SurrealDb>) {
    dotenvy::dotenv().ok();

    let surrealdb_container = SurrealDb::default()
        .with_authentication(false)
        .with_user("backend")
        .with_password("password")
        .with_tag("v2.0.4")
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

    MigrationRunner::new(db.get_inner_ref())
        .up()
        .await
        .expect("Failed to apply migrations");

    let working_request_client = Arc::new(OsuApiRequestClient::new(10));
    let test_request_client = OsuApiTestClient::new(working_request_client.clone(), label);

    let state = AppState::new(test_request_client.clone(), db).await;

    // Requesting peppy to add in our initial database
    let test_initial_user = state
        .credentials_grant_client
        .get_user_osu(2)
        .await
        .unwrap();
    state.db.upsert_user(test_initial_user, true).await.unwrap();

    let routes = test_routes(state.clone()).with_state(state);
    let test_server = TestServer::new(routes).expect("failed to initialize test server");
    (test_server, test_request_client, surrealdb_container)
}
