use serde::{Deserialize, Serialize};

use crate::{error::AppError, osu_api::UserOsu};

use super::{numerical_thing, DatabaseClient};

#[derive(Serialize, Deserialize, Debug)]
pub struct UserDb {
    id: u32,
    username: String,
    avatar_url: String,
    bio: String,
    country: String,
    beatmaps: Vec<u32>,
    mention_count: u32,
}

impl DatabaseClient {
    pub async fn upsert_user(
        &self,
        user_details: UserOsu,
        authorized: bool,
    ) -> Result<(), AppError> {
        let ranked_mapper = user_details.is_ranked_mapper();

        self.db
            .query(
                r#"
                UPSERT $thing 
                SET 
                    username = $username,
                    avatar_url = $avatar_url,
                    country = $country,
                    authorized = $authorized,
                    has_ranked_maps = $ranked_maps;
                "#,
            )
            .bind(("thing", numerical_thing("user", user_details.id)))
            .bind(("username", user_details.username))
            .bind(("avatar_url", user_details.avatar_url))
            .bind(("country", user_details.country))
            .bind(("authorized", authorized.then_some(true)))
            .bind(("ranked_maps", ranked_mapper))
            .await?;
        Ok(())
    }

    pub async fn update_bio(&self, user_id: u32, bio: String) -> Result<(), AppError> {
        self.db
            .query("UPDATE $thing SET bio = $bio")
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("bio", bio))
            .await?;
        Ok(())
    }

    pub async fn add_beatmap_to_user(&self, user_id: u32, beatmap_id: u32) -> Result<(), AppError> {
        self.db
            .query("UPDATE $thing SET beatmaps += $beatmap_id")
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("beatmap_id", beatmap_id))
            .await?;
        Ok(())
    }

    pub async fn remove_beatmap_from_user(
        &self,
        user_id: u32,
        beatmap_id: u32,
    ) -> Result<(), AppError> {
        self.db
            .query("UPDATE $thing SET beatmaps -= $beatmap_id")
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("beatmap_id", beatmap_id))
            .await?;
        Ok(())
    }

    /// TODO: Use the query structure like the one here:
    /// https://surrealdb.com/docs/surrealql/statements/relate#deleting-graph-edges
    pub async fn set_influence_order(&self, user_id: u32, order: &[u32]) -> Result<(), AppError> {
        let enumerated_array: Vec<(u32, u32)> = order
            .iter()
            .enumerate()
            .map(|(index, order)| (index as u32, *order))
            .collect();
        self.db
            .query(
                r#"
                FOR $order in $order_array{
                    UPDATE influenced_by SET order = $order.at(0) 
                    WHERE in = $thing and out = type::thing("user", $order.at(1));
                }
                "#,
            )
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("order_array", enumerated_array))
            .query("UPDATE $thing SET updated_at = time::now()")
            .bind(("thing", numerical_thing("user", user_id)))
            .await?;
        Ok(())
    }

    pub async fn get_user_details(&self, user_id: u32) -> Result<UserDb, AppError> {
        let user_db: Option<UserDb> = self
            .db
            .query(
                "
                SELECT 
                    meta::id(id) as id,
                    username,
                    avatar_url,
                    bio,
                    country,
                    beatmaps,
                    count(<-influenced_by) as mention_count
                FROM ONLY $thing;
                ",
            )
            .bind(("thing", numerical_thing("user", user_id)))
            .await?
            .take(0)?;

        user_db.ok_or(AppError::MissingUser(user_id))
    }
}
