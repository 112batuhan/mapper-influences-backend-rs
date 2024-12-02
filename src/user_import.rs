use std::{sync::Arc, time::Duration};

use mapper_influences_backend_rs::{
    daily_update::update_once,
    database::DatabaseClient,
    osu_api::{credentials_grant::CredentialsGrantClient, request::OsuApiRequestClient},
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let url = std::env::var("SURREAL_URL").expect("Missing SURREAL_URL environment variable");
    let db = DatabaseClient::new(&url)
        .await
        .expect("failed to initialize db connection");

    let users = db.get_users_to_update().await.unwrap();

    let request_client = Arc::new(OsuApiRequestClient::new(100));
    let credentials_grant_client = CredentialsGrantClient::new(request_client).await.unwrap();

    let unsuccessfuls = update_once(
        credentials_grant_client,
        db,
        users,
        Duration::from_millis(300),
    )
    .await;

    dbg!(unsuccessfuls);
}
