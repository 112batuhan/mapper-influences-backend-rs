use serde::{Deserialize, Serialize};
use surrealdb::{
    engine::remote::ws::{Client, Ws, Wss},
    opt::auth::Root,
    sql::{Id, Thing},
    Surreal,
};

use crate::error::AppError;

pub mod activity;
pub mod influence;
pub mod leaderboard;
pub mod user;

pub struct DatabaseClient {
    db: Surreal<Client>,
}

impl DatabaseClient {
    pub async fn new() -> Result<DatabaseClient, AppError> {
        let url = std::env::var("SURREAL_URL").expect("Missing SURREAL_URL environment variable");

        let client = if url.starts_with("wss://") {
            Surreal::new::<Wss>(
                url.strip_prefix("wss://")
                    .expect("starts_with ensures this"),
            )
            .await?
        } else if url.starts_with("ws://") {
            Surreal::new::<Ws>(url.strip_prefix("ws://").expect("starts_with ensures this")).await?
        } else {
            panic!("Badly formatted SURREAL_URL environment variable. Inlude full url with protocol (ws or wss)")
        };

        client
            .signin(Root {
                username: &std::env::var("SURREAL_USER")
                    .expect("Missing SURREAL_USER environment variable"),
                password: &std::env::var("SURREAL_PASS")
                    .expect("Missing SURREAL_PASS envrionment variable"),
            })
            .await?;
        client.use_ns("test").use_db("test").await?;
        Ok(DatabaseClient { db: client })
    }
    pub fn get_inner_ref(&self) -> &Surreal<Client> {
        &self.db
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CustomId {
    id: Id,
}

pub fn numerical_thing(table: &str, number: u32) -> Thing {
    Thing::from((table, Id::Number(number.into())))
}
