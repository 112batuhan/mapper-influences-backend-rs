use std::{net::SocketAddr, sync::Arc};

use axum_test::TestServer;
use mapper_influences_backend_rs::{
    database::DatabaseClient,
    osu_api::{credentials_grant::CredentialsGrantClient, request::OsuApiRequestClient},
    routes, AppState,
};
use osu_test_client::OsuApiTestClient;
use surrealdb_migrations::MigrationRunner;
use testcontainers_modules::{
    surrealdb::{SurrealDb, SURREALDB_PORT},
    testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt},
};

pub mod osu_test_client;

pub async fn init_test_env(
    label: &str,
) -> (TestServer, Arc<OsuApiTestClient>, ContainerAsync<SurrealDb>) {
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

    MigrationRunner::new(db.get_inner_ref())
        .up()
        .await
        .expect("Failed to apply migrations");

    let working_request_client = Arc::new(OsuApiRequestClient::new(10));
    let test_request_client = OsuApiTestClient::new(working_request_client.clone(), label);
    let credentials_grant_client = CredentialsGrantClient::new(test_request_client.clone())
        .await
        .expect("Failed to initialize credentials grant client");

    let state = AppState::new(test_request_client.clone(), credentials_grant_client, db).await;

    // Requesting peppy to add in our initial database
    let test_initial_user = state
        .credentials_grant_client
        .get_user_osu(2)
        .await
        .unwrap();
    state.db.upsert_user(test_initial_user).await.unwrap();

    let routes = routes(state.clone())
        .with_state(state)
        .into_make_service_with_connect_info::<SocketAddr>();

    let test_server = TestServer::new(routes).expect("failed to initialize test server");
    (test_server, test_request_client, surrealdb_container)
}
