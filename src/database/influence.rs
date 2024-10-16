use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::AppError, osu_api::Group};

use super::{numerical_thing, DatabaseClient};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InfluenceDb {
    influenced_by: u32,
    influenced_to: u32,
    influence_type: u8,
    description: String,
    beatmaps: Vec<u32>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MentionsDb {
    id: u32,
    country_code: String,
    country_name: String,
    avatar_url: String,
    username: String,
    groups: Vec<Group>,
    ranked_maps: u32,
    influence_type: u8,
    description: String,
    beatmaps: Vec<u32>,
    mention_count: u32,
}

impl DatabaseClient {
    pub async fn add_influence_relation(
        &self,
        user_id: u32,
        target_user_id: u32,
    ) -> Result<(), AppError> {
        self.db
            .query(
                r#"
                RELATE $user ->influenced_by-> $target
                SET 
                    order = object::values(SELECT COUNT(->influenced_by) FROM ONLY $user).at(0)
                "#,
            )
            .bind(("user", numerical_thing("user", user_id)))
            .bind(("target", numerical_thing("user", target_user_id)))
            .await?;
        Ok(())
    }

    pub async fn remove_influence_relation(
        &self,
        own_user_id: u32,
        target_user_id: u32,
    ) -> Result<(), AppError> {
        self.db
            .query("DELETE $own_user->influenced_by WHERE out=$target_user;")
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .await?;
        Ok(())
    }

    pub async fn add_beatmap_to_influence(
        &self,
        own_user_id: u32,
        target_user_id: u32,
        beatmap_id: u32,
    ) -> Result<(), AppError> {
        self.db
            .query("UPDATE $own_user->influenced_by SET beatmaps += $beatmap_id WHERE out=$target_user;")
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .bind(("beatmap_id", beatmap_id))
            .await?;
        Ok(())
    }

    pub async fn remove_beatmap_from_influence(
        &self,
        own_user_id: u32,
        target_user_id: u32,
        beatmap_id: u32,
    ) -> Result<(), AppError> {
        self.db
            .query("UPDATE $own_user->influenced_by SET beatmaps -= $beatmap_id WHERE out=$target_user;")
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .bind(("beatmap_id", beatmap_id))
            .await?;
        Ok(())
    }

    pub async fn update_influence_type(
        &self,
        own_user_id: u32,
        target_user_id: u32,
        influence_type: u8,
    ) -> Result<(), AppError> {
        self.db
            .query("UPDATE $own_user->influenced_by SET influence_type = $influence_type WHERE out=$target_user;")
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .bind(("influence_type", influence_type))
            .await?;
        Ok(())
    }

    pub async fn update_influence_description(
        &self,
        own_user_id: u32,
        target_user_id: u32,
        description: String,
    ) -> Result<(), AppError> {
        self.db
            .query("UPDATE $own_user->influenced_by SET description=$description WHERE out=$target_user;")
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .bind(("description", description.to_string()))
            .await?;
        Ok(())
    }

    pub async fn get_influences(&self, user_id: u32) -> Result<Vec<InfluenceDb>, AppError> {
        let influences: Vec<InfluenceDb> = self
            .db
            .query(
                "
                SELECT 
                    meta::id(in) as influenced_by,
                    meta::id(out) as influenced_to,
                    influence_type,
                    description,
                    beatmaps,
                    order
                FROM $thing->influenced_by
                ORDER BY order
                ",
            )
            .bind(("thing", numerical_thing("user", user_id)))
            .await?
            .take(0)?;

        Ok(influences)
    }

    pub async fn get_mentions(&self, user_id: u32) -> Result<Vec<MentionsDb>, AppError> {
        let influences: Vec<MentionsDb> = self
            .db
            .query(
                "
                SELECT 
                    meta::id(in) as id,
                    in.country_code as country_code,
                    in.country_name as country_name,
                    in.avatar_url as avatar_url,
                    in.username as username,
                    in.groups as groups,
                    in.ranked_and_approved_beatmapset_count 
                        + in.guest_beatmapset_count as ranked_maps,
                    influence_type,
                    description,
                    beatmaps,
                    COUNT(<-user<-influenced_by) as mention_count
                FROM $thing<-influenced_by 
                ORDER BY mention_count DESC
                ",
            )
            .bind(("thing", numerical_thing("user", user_id)))
            .await?
            .take(0)?;

        Ok(influences)
    }
}
