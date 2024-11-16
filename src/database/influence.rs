use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    osu_api::{BeatmapEnum, OsuBeatmapSmall},
};

use super::{numerical_thing, user::UserSmall, DatabaseClient};

/// `Influence` type. Used in influence and mentions related endpoints
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Clone, Debug)]
pub struct Influence {
    pub user: UserSmall,
    pub influence_type: u8,
    pub description: String,
    /// `OsuUserSmall` type. This array will be empty for mentions endpoint even if the
    /// influence contains beatmaps
    #[serde(default)]
    #[schemars(with = "Vec<OsuBeatmapSmall>")]
    pub beatmaps: Vec<BeatmapEnum>,
}

impl DatabaseClient {
    fn single_influence_return_string(&self) -> &str {
        "
        meta::id(out) as user.id,
        out.username as user.username,
        out.avatar_url as user.avatar_url,
        out.country_code as user.country_code,
        out.country_name as user.country_name,
        out.groups as user.groups,
        out.ranked_and_approved_beatmapset_count 
            + out.guest_beatmapset_count as user.ranked_maps,
        count(out<-influenced_by) as user.mentions,
        beatmaps,
        description,
        influence_type
        "
    }

    pub async fn add_influence_relation(
        &self,
        user_id: u32,
        target_user_id: u32,
    ) -> Result<Influence, AppError> {
        let influence: Option<Influence> = self
            .db
            .query(format!(
                "RELATE $user->influenced_by->$target RETURN {}",
                self.single_influence_return_string()
            ))
            .bind(("user", numerical_thing("user", user_id)))
            .bind(("target", numerical_thing("user", target_user_id)))
            .await?
            .take(0)?;
        influence.ok_or(AppError::MissingInfluence)
    }

    pub async fn remove_influence_relation(
        &self,
        own_user_id: u32,
        target_user_id: u32,
    ) -> Result<Influence, AppError> {
        let influence: Option<Influence> = self
            .db
            .query(format!(
                "
                LET $deleted = DELETE ONLY $own_user->influenced_by WHERE out=$target_user RETURN BEFORE;
                SELECT {} FROM $deleted;
                ",
            self.single_influence_return_string()
        ))
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .await?
            .take(1)?;
        influence.ok_or(AppError::MissingInfluence)
    }

    pub async fn add_beatmap_to_influence(
        &self,
        own_user_id: u32,
        target_user_id: u32,
        beatmap_id: u32,
    ) -> Result<Influence, AppError> {
        let influence: Option<Influence> = self
            .db
            .query(format!(
                "
                UPDATE $own_user->influenced_by SET beatmaps += $beatmap_id WHERE out=$target_user 
                RETURN {}
                ",
                self.single_influence_return_string()
            ))
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .bind(("beatmap_id", beatmap_id))
            .await?
            .take(0)?;
        influence.ok_or(AppError::MissingInfluence)
    }

    pub async fn remove_beatmap_from_influence(
        &self,
        own_user_id: u32,
        target_user_id: u32,
        beatmap_id: u32,
    ) -> Result<Influence, AppError> {
        let influence: Option<Influence> = self
            .db
            .query(format!(
                "
                UPDATE $own_user->influenced_by SET beatmaps -= $beatmap_id WHERE out=$target_user
                RETURN {}
                ",
                self.single_influence_return_string()
            ))
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .bind(("beatmap_id", beatmap_id))
            .await?
            .take(0)?;
        influence.ok_or(AppError::MissingInfluence)
    }

    pub async fn update_influence_type(
        &self,
        own_user_id: u32,
        target_user_id: u32,
        influence_type: u8,
    ) -> Result<Influence, AppError> {
        let influence: Option<Influence> = self
            .db
            .query(format!(
                "
                UPDATE $own_user->influenced_by 
                SET influence_type = $influence_type WHERE out=$target_user
                RETURN {}
                ",
                self.single_influence_return_string()
            ))
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .bind(("influence_type", influence_type))
            .await?
            .take(0)?;
        influence.ok_or(AppError::MissingInfluence)
    }

    pub async fn update_influence_description(
        &self,
        own_user_id: u32,
        target_user_id: u32,
        description: String,
    ) -> Result<Influence, AppError> {
        let influence: Option<Influence> = self
            .db
            .query(format!(
                "
                UPDATE $own_user->influenced_by
                SET description=$description WHERE out=$target_user
                RETURN {}
                ",
                self.single_influence_return_string()
            ))
            .bind(("own_user", numerical_thing("user", own_user_id)))
            .bind(("target_user", numerical_thing("user", target_user_id)))
            .bind(("description", description.to_string()))
            .await?
            .take(0)?;
        influence.ok_or(AppError::MissingInfluence)
    }

    pub async fn get_influences(
        &self,
        user_id: u32,
        start: u32,
        limit: u32,
    ) -> Result<Vec<Influence>, AppError> {
        let influences: Vec<Influence> = self
            .db
            .query(
                "
                SELECT 
                    meta::id(out) as user.id,
                    out.country_code as user.country_code,
                    out.country_name as user.country_name,
                    out.avatar_url as user.avatar_url,
                    out.username as user.username,
                    out.groups as user.groups,
                    out.ranked_and_approved_beatmapset_count 
                        + out.guest_beatmapset_count as user.ranked_maps,
                    COUNT(->user<-influenced_by) as user.mentions,
                    influence_type,
                    description,
                    beatmaps,
                    order
                FROM $thing->influenced_by
                ORDER BY order
                START $start
                LIMIT $limit
                ",
            )
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("limit", limit))
            .bind(("start", start))
            .await?
            .take(0)?;

        Ok(influences)
    }

    pub async fn get_mentions(
        &self,
        user_id: u32,
        start: u32,
        limit: u32,
    ) -> Result<Vec<Influence>, AppError> {
        let influences: Vec<Influence> = self
            .db
            .query(
                "
                SELECT 
                    meta::id(in) as user.id,
                    in.country_code as user.country_code,
                    in.country_name as user.country_name,
                    in.avatar_url as user.avatar_url,
                    in.username as user.username,
                    in.groups as user.groups,
                    in.ranked_and_approved_beatmapset_count 
                        + in.guest_beatmapset_count as user.ranked_maps,
                    COUNT(<-user<-influenced_by) as user.mentions,
                    influence_type,
                    description
                FROM $thing<-influenced_by 
                ORDER BY user.mentions DESC
                START $start
                LIMIT $limit
                ",
            )
            .bind(("thing", numerical_thing("user", user_id)))
            .bind(("limit", limit))
            .bind(("start", start))
            .await?
            .take(0)?;

        Ok(influences)
    }
}
