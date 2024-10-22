use futures::future::join_all;
use mapper_influences_backend_rs::database::{numerical_thing, DatabaseClient};
use mapper_influences_backend_rs::osu_api::Group;
use ordermap::OrderSet;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::thread::sleep_ms;
use std::time::Duration;
use surrealdb::sql::Thing;
use surrealdb_migrations::MigrationRunner;

fn deserialize_beatmap_ids<'de, D>(deserializer: D) -> Result<Vec<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Beatmap {
        id: i64,
    }
    let beatmaps = Vec::<Beatmap>::deserialize(deserializer)?;
    Ok(beatmaps.into_iter().map(|b| b.id).collect())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    id: u32,
    username: String,
    avatar_url: String,
    #[serde(deserialize_with = "deserialize_beatmap_ids")]
    #[serde(default)]
    beatmaps: Vec<i64>,
    bio: Option<String>,
    #[serde(rename(serialize = "ranked_mapper"))]
    have_ranked_map: bool,
    #[serde(default)]
    #[serde(skip_serializing)]
    influence_order: OrderSet<i64>,
    #[serde(rename(deserialize = "country"))]
    country_code: String,
    #[serde(default)]
    country_name: String,
    #[serde(default = "default_groups")]
    groups: Vec<Group>,
    #[serde(default)]
    previous_usernames: Vec<String>,
    #[serde(default)]
    ranked_and_approved_beatmapset_count: u32,
    #[serde(default)]
    ranked_beatmapset_count: u32,
    #[serde(default)]
    nominated_beatmapset_count: u32,
    #[serde(default)]
    guest_beatmapset_count: u32,
    #[serde(default)]
    loved_beatmapset_count: u32,
    #[serde(default)]
    graveyard_beatmapset_count: u32,
    #[serde(default)]
    pending_beatmapset_count: u32,
}

fn default_groups() -> Vec<Group> {
    vec![Group {
        colour: Some("a".to_string()),
        name: "b".to_string(),
        short_name: "c".to_string(),
    }]
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserFull {
    #[serde(flatten)]
    user: User,
    authenticated: bool,
}

#[derive(Deserialize, Debug)]
pub struct Influence {
    influenced_by: u32,
    influenced_to: u32,
    #[serde(deserialize_with = "deserialize_beatmap_ids")]
    #[serde(default)]
    beatmaps: Vec<i64>,
    #[serde(rename(deserialize = "type"))]
    influence_type: u8,
    description: String,
}

#[derive(Serialize, Debug)]
pub struct InfluenceWithReferences {
    r#in: Thing,
    out: Thing,
    beatmaps: Vec<i64>,
    influence_type: u8,
    description: String,
}
impl From<Influence> for InfluenceWithReferences {
    fn from(influence: Influence) -> Self {
        InfluenceWithReferences {
            r#in: numerical_thing("user", influence.influenced_by),
            out: numerical_thing("user", influence.influenced_to),
            beatmaps: influence.beatmaps,
            influence_type: influence.influence_type,
            description: influence.description,
        }
    }
}

fn read_json_file<T>(file_path: &str) -> T
where
    T: DeserializeOwned,
{
    let file = File::open(file_path).unwrap();
    let reader = BufReader::new(file);
    let data: T = serde_json::from_reader(reader).unwrap();
    data
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let path = "./conversion/users.json";
    let users: Vec<User> = read_json_file(path);

    // well there is no point of using this since we stopped recording that table.
    // this was a really cheap way to record how many accounts we had.
    // Now I'm moving this to user table
    let path = "./conversion/real_users.json";
    let real_users: Vec<User> = read_json_file(path);

    let real_user_ids: Vec<u32> = real_users.into_iter().map(|user| user.id).collect();
    let full_users: Vec<UserFull> = users
        .into_iter()
        .map(|user| {
            let authenticated = real_user_ids.contains(&user.id)
                || !user.beatmaps.is_empty()
                || user.bio.clone().is_some_and(|bio| !bio.is_empty());
            UserFull {
                authenticated,
                user,
            }
        })
        .collect();

    let path = "./conversion/influences.json";
    let influences: Vec<Influence> = read_json_file(path);

    let db = DatabaseClient::new().await.unwrap();

    // WARN: BE EXTREMELY CAUTIOUS WITH THIS!!!!
    // YOU MIGHT ACCIDENTALLY DELETE PROD DATA
    // resetting DB so that we don't get duplicate results
    // TODO: run this only when env variables are `test`
    db.get_inner_ref()
        .query("REMOVE NAMESPACE test")
        .await
        .unwrap();

    MigrationRunner::new(db.get_inner_ref())
        .up()
        .await
        .expect("Failed to apply migrations");
    println!("Migration done");

    db.get_inner_ref()
        .query("INSERT INTO user ($values)")
        .bind(("values", full_users.clone()))
        .await
        .unwrap();
    println!("User insertion done");

    let db_influences: Vec<InfluenceWithReferences> = influences
        .into_iter()
        .map(InfluenceWithReferences::from)
        .collect();

    db.get_inner_ref()
        .query("INSERT RELATION INTO influenced_by ($values)")
        .bind(("values", db_influences))
        .await
        .unwrap();

    println!("Influence insertion done");

    // WARN: BE EXTREMELY CAUTIOUS WITH THIS!!!!
    // YOU MIGHT ACCIDENTALLY DELETE PROD DATA
    // Deleting ADD_INFLUENCE events after adding data.
    db.get_inner_ref().query("delete activity").await.unwrap();

    let mut handlers = Vec::new();
    let arc_db = Arc::new(db);
    for user in full_users {
        let order_vec: Vec<u32> = user
            .user
            .influence_order
            .iter()
            .copied()
            .map(|number| number as u32)
            .collect();

        let task_db = Arc::clone(&arc_db);

        let handler = tokio::spawn(async move {
            let order_vec = order_vec;
            task_db
                .set_influence_order(user.user.id, &order_vec)
                .await
                .unwrap();
        });
        handlers.push(handler);
    }
    join_all(handlers).await;
    println!("custom order insertion done");
    println!("done");
}
