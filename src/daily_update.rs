use std::{sync::Arc, time::Duration};

use crate::{
    database::DatabaseClient, osu_api::credentials_grant::CredentialsGrantClient, retry::Retryable,
};

pub async fn update_once(
    client: Arc<CredentialsGrantClient>,
    database: Arc<DatabaseClient>,
    users_to_update: Vec<u32>,
    wait_duration: Duration,
) {
    let mut interval = tokio::time::interval(wait_duration);
    for user_id in users_to_update {
        interval.tick().await;
        let Ok(user) = client.get_user_osu(user_id).await else {
            tracing::error!(
                "Failed to request {} from osu! API for daily update",
                user_id
            );
            continue;
        };
        let Ok(_) = database.upsert_user(user).await else {
            tracing::error!(
                "Failed to insert user {} to database for daily update",
                user_id
            );
            continue;
        };
        tracing::debug!("Requested and inserted user {} for daily update", user_id);
    }
}

pub async fn update_routine(
    client: Arc<CredentialsGrantClient>,
    mut database: Arc<DatabaseClient>,
    initial_sleep_time: Duration,
) {
    tokio::time::sleep(initial_sleep_time).await;
    let mut interval = tokio::time::interval(Duration::from_secs(60 * 60 * 24));
    loop {
        interval.tick().await;
        let users_to_update: Vec<u32> = database
            .retry_until_success(60, "Failed to fetch users for daily update")
            .await;
        update_once(
            client.clone(),
            database.clone(),
            users_to_update,
            Duration::from_secs(15),
        )
        .await;
    }
}
