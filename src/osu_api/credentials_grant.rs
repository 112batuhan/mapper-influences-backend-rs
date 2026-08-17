use std::{sync::Arc, time::Duration};

use tokio::{
    sync::{watch, OnceCell},
    time::sleep,
};

use crate::{error::AppError, retry::Retryable};

use super::{request::Requester, UserOsu};

/// Seconds shaved off a token's lifetime so we refresh before it actually expires.
const REFRESH_BUFFER_SECS: u64 = 120;
/// Lower bound on the refresh interval. Guards against a short-lived (or misreported) token
/// spinning us into a tight refresh loop, and against underflow when subtracting the buffer.
const MIN_REFRESH_SECS: u64 = 60;

/// A wrapper to [`Requester`] that stores and periodically refreshes the client-credentials-grant
/// token used for activity, leaderboard and daily update requests.
pub struct CredentialsGrantClient {
    client: Arc<dyn Requester>,
    /// Latest token. `None` until the refresh loop produces the first one.
    token: watch::Sender<Option<String>>,
    /// Ensures the background refresh loop is spawned exactly once, lazily on first use.
    start: OnceCell<()>,
}

impl CredentialsGrantClient {
    pub async fn new(client: Arc<dyn Requester>) -> Result<Arc<CredentialsGrantClient>, AppError> {
        // Keep one receiver alive so the channel isn't considered closed before the loop starts.
        let (token, _keep_alive) = watch::channel(None);
        Ok(Arc::new(CredentialsGrantClient {
            client,
            token,
            start: OnceCell::new(),
        }))
    }

    /// Spawn the refresh loop on first use. [`OnceCell`] guarantees a single spawn even when
    /// several callers race here concurrently, so there is no check-then-act window.
    async fn ensure_loop_started(&self) {
        self.start
            .get_or_init(|| async {
                let mut client: Arc<dyn Requester> = self.client.clone();
                let token_tx = self.token.clone();
                tokio::spawn(async move {
                    // We can't fail this task; the best we can do is retry. If it can't get a
                    // token, the rest of the app probably can't reach osu! either.
                    loop {
                        let new_token = client
                            .retry_until_success(60, "Failed to get client credentials grant token")
                            .await;
                        let expires_in = new_token.expires_in as u64;
                        // If every receiver is gone the app is shutting down; stop refreshing.
                        if token_tx.send(Some(new_token.access_token)).is_err() {
                            break;
                        }
                        // Refresh based on *this* token's lifetime, clamped so we never underflow
                        // the buffer or busy-loop on an implausibly short expiry.
                        let refresh_in = expires_in
                            .saturating_sub(REFRESH_BUFFER_SECS)
                            .max(MIN_REFRESH_SECS);
                        sleep(Duration::from_secs(refresh_in)).await;
                    }
                });
            })
            .await;
    }

    pub fn get_token_only(&self) -> Result<Option<String>, AppError> {
        Ok(self.token.borrow().clone())
    }

    /// Returns the current token, lazily starting the refresh loop and waiting for the first token
    /// if one hasn't been produced yet.
    pub async fn get_access_token(&self) -> Result<String, AppError> {
        if let Some(token) = self.token.borrow().clone() {
            return Ok(token);
        }
        self.ensure_loop_started().await;

        // Wait until the loop publishes a token. The sender lives inside `self`, so this can only
        // error if `self` was dropped, which cannot happen while we hold `&self`.
        let mut receiver = self.token.subscribe();
        receiver
            .wait_for(|token| token.is_some())
            .await
            .map_err(|_| AppError::CredentialsTokenUnavailable)?;

        self.token
            .borrow()
            .clone()
            .ok_or(AppError::CredentialsTokenUnavailable)
    }

    /// Ease of use to get user data since we already contain the client inside
    pub async fn get_user_osu(&self, user_id: u32) -> Result<UserOsu, AppError> {
        let token = self.get_access_token().await?;
        self.client.get_user_osu(&token, user_id).await
    }
}
