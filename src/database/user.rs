use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

use crate::{
    error::AppError,
    osu_api::{Group, UserOsu},
};

use super::{numerical_thing, DatabaseClient};

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
pub struct UserWithoutBeatmap {
    pub id: u32,
    pub username: String,
    pub avatar_url: String,
    pub bio: String,
    pub groups: Vec<Group>,
    pub country_code: String,
    pub country_name: String,
    pub previous_usernames: Vec<String>,
    pub ranked_and_approved_beatmapset_count: u32,
    pub ranked_beatmapset_count: u32,
    pub nominated_beatmapset_count: u32,
    pub guest_beatmapset_count: u32,
    pub loved_beatmapset_count: u32,
    pub graveyard_beatmapset_count: u32,
    pub pending_beatmapset_count: u32,
}

impl From<UserOsu> for UserWithoutBeatmap {
    fn from(user_osu: UserOsu) -> Self {
        UserWithoutBeatmap {
            id: user_osu.id,
            username: user_osu.username,
            avatar_url: user_osu.avatar_url,
            bio: "".to_string(),
            groups: user_osu.groups,
            country_code: user_osu.country.code,
            country_name: user_osu.country.name,
            previous_usernames: user_osu.previous_usernames,
            ranked_and_approved_beatmapset_count: user_osu.ranked_and_approved_beatmapset_count,
            ranked_beatmapset_count: user_osu.ranked_beatmapset_count,
            nominated_beatmapset_count: user_osu.nominated_beatmapset_count,
            guest_beatmapset_count: user_osu.guest_beatmapset_count,
            loved_beatmapset_count: user_osu.loved_beatmapset_count,
            graveyard_beatmapset_count: user_osu.graveyard_beatmapset_count,
            pending_beatmapset_count: user_osu.pending_beatmapset_count,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct UserDb {
    #[serde(flatten)]
    pub data: UserWithoutBeatmap,
    pub beatmaps: Vec<u32>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct UserCondensed {
    pub id: u32,
    pub username: String,
    pub avatar_url: String,
    pub groups: Vec<Group>,
    pub country_code: String,
    pub country_name: String,
    pub ranked_maps: u32,
}

impl From<UserOsu> for UserCondensed {
    fn from(user: UserOsu) -> Self {
        UserCondensed {
            id: user.id,
            username: user.username,
            avatar_url: user.avatar_url,
            groups: user.groups,
            country_code: user.country.code,
            country_name: user.country.name,
            ranked_maps: user.ranked_and_approved_beatmapset_count + user.guest_beatmapset_count,
        }
    }
}

impl DatabaseClient {
    pub async fn upsert_user(
        &self,
        user_details: UserOsu,
        authenticated: bool,
    ) -> Result<(), AppError> {
        let ranked_mapper = user_details.is_ranked_mapper();
        self.db
            .query(
                r#"
                UPSERT $thing 
                SET 
                    username = $username,
                    avatar_url = $avatar_url,
                    authenticated = $authenticated,
                    ranked_mapper = $ranked_maps,
                    country_code = $country_code,
                    country_name = $country_name,
                    groups = $groups,
                    previous_usernames = $previous_usernames,
                    ranked_and_approved_beatmapset_count = $ranked_and_approved_beatmapset_count,
                    ranked_beatmapset_count = $ranked_beatmapset_count,
                    nominated_beatmapset_count = $nominated_beatmapset_count,
                    guest_beatmapset_count = $guest_beatmapset_count,
                    loved_beatmapset_count = $loved_beatmapset_count,
                    graveyard_beatmapset_count = $graveyard_beatmapset_count,
                    pending_beatmapset_count = $pending_beatmapset_count;
                "#,
            )
            .bind(("thing", numerical_thing("user", user_details.id)))
            .bind(("username", user_details.username))
            .bind(("avatar_url", user_details.avatar_url))
            .bind(("authenticated", authenticated.then_some(true)))
            .bind(("ranked_maps", ranked_mapper))
            .bind(("country_code", user_details.country.code))
            .bind(("country_name", user_details.country.name))
            .bind(("groups", user_details.groups))
            .bind(("previous_usernames", user_details.previous_usernames))
            .bind((
                "ranked_and_approved_beatmapset_count",
                user_details.ranked_and_approved_beatmapset_count,
            ))
            .bind((
                "ranked_beatmapset_count",
                user_details.ranked_beatmapset_count,
            ))
            .bind((
                "nominated_beatmapset_count",
                user_details.nominated_beatmapset_count,
            ))
            .bind((
                "guest_beatmapset_count",
                user_details.guest_beatmapset_count,
            ))
            .bind((
                "loved_beatmapset_count",
                user_details.loved_beatmapset_count,
            ))
            .bind((
                "graveyard_beatmapset_count",
                user_details.graveyard_beatmapset_count,
            ))
            .bind((
                "pending_beatmapset_count",
                user_details.pending_beatmapset_count,
            ))
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
                    UPDATE $thing->influenced_by SET order = $order.at(0) 
                    WHERE out = type::thing("user", $order.at(1));
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
                    beatmaps,
                    country_code,
                    country_name,
                    groups,
                    previous_usernames,
                    ranked_and_approved_beatmapset_count,
                    ranked_beatmapset_count,
                    nominated_beatmapset_count,
                    guest_beatmapset_count,
                    loved_beatmapset_count,
                    graveyard_beatmapset_count,
                    pending_beatmapset_count
                FROM ONLY $thing;
                ",
            )
            .bind(("thing", numerical_thing("user", user_id)))
            .await?
            .take(0)?;

        user_db.ok_or(AppError::MissingUser(user_id))
    }

    pub async fn get_multiple_user_details(
        &self,
        user_ids: &[u32],
    ) -> Result<Vec<UserCondensed>, AppError> {
        let things: Vec<Thing> = user_ids
            .iter()
            .map(|id| numerical_thing("user", *id))
            .collect();
        let users_db: Vec<UserCondensed> = self
            .db
            .query(
                "
                SELECT 
                    meta::id(id) as id,
                    username,
                    avatar_url,
                    country_code,
                    country_name,
                    groups,
                    ranked_and_approved_beatmapset_count 
                        + guest_beatmapset_count as ranked_maps
                FROM $things;
                ",
            )
            .bind(("things", things))
            .await?
            .take(0)?;
        Ok(users_db)
    }
}
