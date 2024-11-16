use std::{
    ops::DerefMut,
    sync::{Arc, RwLock},
    time::Duration,
};

use tokio::{sync::oneshot, sync::Mutex as AsyncMutex, time::sleep};

use crate::{error::AppError, retry::Retryable};

use super::{request::Requester, UserOsu};

/// A wrapper to [`RequestClient`] to store and update credentials grant client auth method token
///
/// Will be used to request activity, leaderboard and daily update data
pub struct CredentialsGrantClient {
    client: Arc<dyn Requester>,
    token: RwLock<Option<String>>,
    // To start the loop lazily
    start_sender: AsyncMutex<Option<oneshot::Sender<()>>>,
    end_receiver: AsyncMutex<Option<oneshot::Receiver<()>>>,
}

impl CredentialsGrantClient {
    pub async fn new(client: Arc<dyn Requester>) -> Result<Arc<CredentialsGrantClient>, AppError> {
        let (start_sender, start_receiver) = oneshot::channel();
        let (end_sender, end_receiver) = oneshot::channel();
        let client = Arc::new(CredentialsGrantClient {
            client,
            token: RwLock::new(None),
            start_sender: AsyncMutex::new(Some(start_sender)),
            end_receiver: AsyncMutex::new(Some(end_receiver)),
        });
        client.clone().start_loop(start_receiver, end_sender);
        Ok(client)
    }

    fn update_token(&self, new_token: String) -> Result<(), AppError> {
        let mut token = self.token.write().map_err(|_| AppError::RwLock)?;
        *token = Some(new_token);
        Ok(())
    }

    // I could refactor the retry and update functions but whatever.
    fn start_loop(
        self: Arc<Self>,
        start_receiver: oneshot::Receiver<()>,
        end_sender: oneshot::Sender<()>,
    ) {
        let buffer_time = 120;
        let mut client_clone = self.client.clone();

        // we can't fail this task, best we can do is to retry. If this doesn't work,
        // then there is a good chance that the rest of the requests won't work either
        tokio::spawn(async move {
            let _ = start_receiver.await;
            let token = client_clone
                .retry_until_success(60, "Failed to get client credentials grant token")
                .await;
            let _ = self.update_token(token.access_token);
            let _ = end_sender.send(());
            loop {
                sleep(Duration::from_secs(token.expires_in as u64 - buffer_time)).await;
                let token = client_clone
                    .retry_until_success(60, "Failed to get client credentials grant token")
                    .await;
                let _ = self.update_token(token.access_token);
            }
        });
    }

    pub fn get_token_only(&self) -> Result<Option<String>, AppError> {
        let token_guard = self.token.read().map_err(|_| AppError::RwLock)?;
        Ok(token_guard.clone())
    }

    /// Starting the loop lazily after the first token access.
    /// This is necessary for tests. We don't want to request token if we don't need to.
    pub async fn get_access_token(&self) -> Result<String, AppError> {
        if let Some(token) = self.get_token_only()? {
            Ok(token)
        } else {
            // this is a good place to panic. There is no way for the sender and receivers to drop.
            // If it does, then rest of the app probably isn't working
            self.start_sender
                .lock()
                .await
                .deref_mut()
                .take()
                .expect("start sender is missing")
                .send(())
                .expect("Failed to send start message");

            self.end_receiver
                .lock()
                .await
                .deref_mut()
                .take()
                .expect("end receiver is missing")
                .await
                .expect("Failed receive end message");
            let token_guard = self.token.read().map_err(|_| AppError::RwLock)?;
            let Some(token) = token_guard.clone() else {
                panic!("Failed to initialize client grant token")
            };
            Ok(token)
        }
    }

    /// Ease of use to get user data since we already contain the client inside
    pub async fn get_user_osu(&self, user_id: u32) -> Result<UserOsu, AppError> {
        let token = self.get_access_token().await?;
        self.client.get_user_osu(&token, user_id).await
    }
}
