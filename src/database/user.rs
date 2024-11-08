use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

use crate::{
    error::AppError,
    osu_api::{BeatmapEnum, Group, UserOsu},
};

use super::{numerical_thing, DatabaseClient};

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
pub struct User {
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
    pub beatmaps: Vec<BeatmapEnum>,
    pub mentions: Option<u32>,
}

impl From<UserOsu> for User {
    fn from(user_osu: UserOsu) -> Self {
        User {
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
            beatmaps: Vec::new(),
            mentions: None,
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
pub struct UserSmall {
    pub id: u32,
    pub username: String,
    pub avatar_url: String,
    pub groups: Vec<Group>,
    pub country_code: String,
    pub country_name: String,
    pub ranked_maps: u32,
    pub mentions: Option<u32>,
}

impl From<UserOsu> for UserSmall {
    fn from(user: UserOsu) -> Self {
        UserSmall {
            id: user.id,
            username: user.username,
            avatar_url: user.avatar_url,
            groups: user.groups,
            country_code: user.country.code,
            country_name: user.country.name,
            ranked_maps: user.ranked_and_approved_beatmapset_count + user.guest_beatmapset_count,
            mentions: None,
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

    fn single_user_return_string(&self) -> &str {
        "
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
        pending_beatmapset_count,
        count(<-influenced_by) as mentions
        "
    }

    pub async fn update_bio(&self, user_id: u32, bio: String) -> Result<User, AppError> {
        let user: Option<User> = self
            .db
            .query(format!(
                "UPDATE $thing SET bio = $bio RETURN {}",
                self.single_user_return_string()
            ))
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("bio", bio))
            .await?
            .take(0)?;

        user.ok_or(AppError::MissingUser(user_id))
    }

    pub async fn add_beatmap_to_user(
        &self,
        user_id: u32,
        beatmap_id: u32,
    ) -> Result<User, AppError> {
        let user: Option<User> = self
            .db
            .query(format!(
                "UPDATE $thing SET beatmaps += $beatmap_id RETURN {}",
                self.single_user_return_string()
            ))
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("beatmap_id", beatmap_id))
            .await?
            .take(0)?;

        user.ok_or(AppError::MissingUser(user_id))
    }

    pub async fn remove_beatmap_from_user(
        &self,
        user_id: u32,
        beatmap_id: u32,
    ) -> Result<User, AppError> {
        let user: Option<User> = self
            .db
            .query(format!(
                "UPDATE $thing SET beatmaps -= $beatmap_id RETURN {}",
                self.single_user_return_string()
            ))
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("beatmap_id", beatmap_id))
            .await?
            .take(0)?;
        user.ok_or(AppError::MissingUser(user_id))
    }

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

    pub async fn get_user_details(&self, user_id: u32) -> Result<User, AppError> {
        let user: Option<User> = self
            .db
            .query(format!(
                "SELECT {} FROM ONLY $thing;",
                self.single_user_return_string()
            ))
            .bind(("thing", numerical_thing("user", user_id)))
            .await?
            .take(0)?;

        user.ok_or(AppError::MissingUser(user_id))
    }

    pub async fn get_multiple_user_details(
        &self,
        user_ids: &[u32],
    ) -> Result<Vec<UserSmall>, AppError> {
        let things: Vec<Thing> = user_ids
            .iter()
            .map(|id| numerical_thing("user", *id))
            .collect();
        let users: Vec<UserSmall> = self
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
                    count(<-influenced_by) as mentions
                FROM $things;
                ",
            )
            .bind(("things", things))
            .await?
            .take(0)?;
        Ok(users)
    }
}
