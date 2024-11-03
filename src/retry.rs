use std::{error::Error, time::Duration};

use futures::Future;

pub trait Retryable {
    type Value: Send;
    type Err: Error + Send;
    fn retry(&mut self) -> impl Future<Output = Result<Self::Value, Self::Err>> + Send;
    fn retry_until_success(
        &mut self,
        longest_cooldown: u32,
        message: &str,
    ) -> impl Future<Output = Self::Value> + Send
    where
        Self: Send,
    {
        async move {
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

                        let cooldown = if cooldown > longest_cooldown {
                            longest_cooldown
                        } else {
                            let fibo_temp = cooldown;
                            cooldown += cooldown_fibo_last;
                            cooldown_fibo_last = fibo_temp;
                            cooldown
                        };
                        attempt += 1;
                        tokio::time::sleep(Duration::from_secs(cooldown.into())).await;
                    }
                }
            }
        }
    }
}
