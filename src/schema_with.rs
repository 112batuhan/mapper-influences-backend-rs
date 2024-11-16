use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{database::user::UserSmall, osu_api::BeatmapEnum};

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct FlattenedActivityType {
    pub event_type: EventType,
    /// Influenced user. For `ADD_INFLUENCE`, `REMOVE_INFLUENCE`,
    /// `ADD_INFLUENCE_BEATMAP`, `REMOVE_INFLUENCE_BEATMAP`, `EDIT_INFLUENCE_DESC`,
    /// `EDIT_INFLUENCE_TYPE` activity types.
    pub influence: Option<UserSmall>,
    /// Added or removed beatmap. for `ADD_USER_BEATMAP`, `REMOVE_USER_BEATMAP`,
    /// `ADD_INFLUENCE_BEATMAP`, `REMOVE_INFLUENCE_BEATMAP` activity types.
    pub beatmap: Option<BeatmapEnum>,
    /// Changed influence description. for `EDIT_INFLUENCE_DESC` activity type.
    pub description: Option<String>,
    /// Changed influence type. for `EDIT_INFLUENCE_TYPE` activity type.
    pub influence_type: Option<u8>,
    /// Changed bio. For `EDIT_BIO` activity type.
    pub bio: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Login,
    AddInfluence,
    RemoveInfluence,
    AddUserBeatmap,
    RemoveUserBeatmap,
    AddInfluenceBeatmap,
    RemoveInfluenceBeatmap,
    EditInfluenceDesc,
    EditInfluenceType,
    EditBio,
}
