use async_trait::async_trait;
use std::{error::Error, time::Duration};

#[async_trait]
pub trait Retryable<Value: Send + Sync, Err: Error + Send>: Send {
    async fn retry(&mut self) -> Result<Value, Err>;
    async fn retry_until_success(&mut self, longest_cooldown: u32, message: &str) -> Value {
        let mut cooldown_fibo_last = 0;
        let mut cooldown = 1;
        let mut attempt = 1;
        loop {
            match self.retry().await {
                Ok(value) => {
                    return value;
                }
                Err(error) => {
                    tracing::error!(
                        "{}. Trying to reconnect. Attempt {}, Cooldown {} secs. full error: {}",
                        message,
                        attempt,
                        cooldown,
                        error
                    );
                    let fibo_temp = cooldown;
                    cooldown += cooldown_fibo_last;
                    if cooldown > longest_cooldown {
                        cooldown = longest_cooldown;
                    }
                    cooldown_fibo_last = fibo_temp;
                    attempt += 1;
                    tokio::time::sleep(Duration::from_secs(cooldown.into())).await;
                }
            }
        }
    }
}
