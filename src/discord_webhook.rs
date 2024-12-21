use http::StatusCode;
use webhook::models::Message;

use crate::error::AppError;

pub struct WebhookClient {
    client: reqwest::Client,
    url: String,
}

impl WebhookClient {
    pub fn new(url: &str) -> WebhookClient {
        WebhookClient {
            client: reqwest::Client::new(),
            url: url.to_owned(),
        }
    }

    /// Basically a simple recreation of webhook-rs client send implementation with reqwest
    pub async fn send(&self, message: &Message) -> Result<(), AppError> {
        let res = self.client.post(&self.url).json(message).send().await?;
        if res.status() == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            let err_msg = match res.text().await {
                Ok(msg) => msg,
                Err(err) => format!("Webhook reqwest client error: {}", err),
            };
            Err(AppError::Webhook(err_msg))
        }
    }
}
