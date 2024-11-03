use std::{error::Error, sync::Arc, time::Duration};

use futures::Future;

pub trait Retryable {
    type Value;
    type Err: Error;
    fn retry(&mut self) -> impl Future<Output = Result<Self::Value, RetryAction<Self::Err>>>;
}

pub enum RetryOption {
    Continue,
    Retry,
}
pub struct RetryAction<E: Error> {
    error: E,
    message: String,
    retry_option: RetryOption,
}

impl<E: Error> RetryAction<E> {
    pub fn new(error: E, message: String, retry_option: RetryOption) -> Self {
        Self {
            error,
            message,
            retry_option,
        }
    }
}

pub struct Retry<T: Retryable> {
    cooldown_fibo_last: u32,
    cooldown: u32,
    attempt: u32,
    longest_cooldown: u32,
    retryable: T,
}

impl<T: Retryable> Retry<T> {
    pub fn new(longest_cooldown: u32, retryable: T) -> Self {
        Retry {
            cooldown_fibo_last: 0,
            cooldown: 1,
            attempt: 1,
            longest_cooldown,
            retryable,
        }
    }

    pub async fn retry_until_success(&mut self) -> <T as Retryable>::Value {
        loop {
            match self.retryable.retry().await {
                Ok(value) => {
                    self.cooldown_fibo_last = 0;
                    self.cooldown = 1;
                    self.attempt = 1;
                    return value;
                }
                Err(retry) => match retry.retry_option {
                    RetryOption::Continue => {
                        tracing::warn!(
                            "{}. Continuing. full error: {}",
                            &retry.message,
                            retry.error
                        );
                    }
                    RetryOption::Retry => {
                        tracing::error!(
                            "{}. Trying to reconnect. Attempt {}, Cooldown {} secs. full error: {}",
                            &retry.message,
                            &self.attempt,
                            &self.cooldown,
                            retry.error
                        );

                        let cooldown = if self.cooldown > self.longest_cooldown {
                            self.longest_cooldown
                        } else {
                            let fibo_temp = self.cooldown;
                            self.cooldown += self.cooldown_fibo_last;
                            self.cooldown_fibo_last = fibo_temp;
                            self.cooldown
                        };

                        tokio::time::sleep(Duration::from_secs(cooldown.into())).await;
                    }
                },
            }
        }
    }
}
