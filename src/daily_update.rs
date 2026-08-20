use std::{sync::Arc, time::Duration};

use crate::{
    database::DatabaseClient, error::AppError, osu_api::credentials_grant::CredentialsGrantClient,
    retry::Retryable,
};

/// Returns the ids that are worth trying again on the next run. Users the osu! API has stopped
/// handing out are not among them, they are dealt with in place, see [`update_once`].
pub async fn update_once(
    client: Arc<CredentialsGrantClient>,
    database: Arc<DatabaseClient>,
    users_to_update: Vec<u32>,
    wait_duration: Duration,
) -> Vec<u32> {
    let mut interval = tokio::time::interval(wait_duration);

    let mut unsuccessfull_ids = Vec::new();
    for user_id in users_to_update {
        interval.tick().await;
        let user = match client.get_user_osu(user_id).await {
            Ok(user) => user,
            // Restricted and deleted users stay in our database, but the osu! API stops returning
            // them. There is no data to insert, so their update date is pushed forward instead,
            // which takes them out of the window until it comes back around. That keeps them out
            // of every run in between without giving up on them: if the restriction is lifted,
            // the next window picks them up with fresh data.
            Err(AppError::OsuApiStatus(404)) => {
                if let Err(error) = database.update_user_updated_at(user_id).await {
                    unsuccessfull_ids.push(user_id);
                    tracing::error!(
                        "Failed to update the date of missing user {} for daily update: {}",
                        user_id,
                        error
                    );
                    continue;
                }
                tracing::debug!(
                    "Skipped user {} for daily update, the osu! API no longer returns them",
                    user_id
                );
                continue;
            }
            // Anything else is a genuine failure, a rate limit or our own token going bad. Left
            // in the window so the next run tries again.
            Err(error) => {
                unsuccessfull_ids.push(user_id);
                tracing::error!(
                    "Failed to request {} from osu! API for daily update: {}",
                    user_id,
                    error
                );
                continue;
            }
        };
        let Ok(_) = database.upsert_user(user).await else {
            unsuccessfull_ids.push(user_id);
            tracing::error!(
                "Failed to insert user {} to database for daily update",
                user_id
            );
            continue;
        };
        tracing::debug!("Requested and inserted user {} for daily update", user_id);
    }
    unsuccessfull_ids
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
            Duration::from_secs(60),
        )
        .await;
    }
}
