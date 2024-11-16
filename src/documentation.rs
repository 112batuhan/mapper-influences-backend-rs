//! Custom documentation types and wrappers

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{database::user::UserSmall, osu_api::OsuBeatmapSmall};

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct FlattenedActivityType {
    pub event_type: EventType,
    pub influence: Option<UserSmallActivity>,
    pub beatmap: Option<OsuBeatmapSmallActivity>,
    /// Changed influence description. for `EDIT_INFLUENCE_DESC` activity type.
    pub description: Option<String>,
    /// Changed influence type. for `EDIT_INFLUENCE_TYPE` activity type.
    pub influence_type: Option<u8>,
    /// Changed bio. For `EDIT_BIO` activity type.
    pub bio: Option<String>,
}

/// Influenced user. `UserSmall` type. For `ADD_INFLUENCE`, `REMOVE_INFLUENCE`,
/// `ADD_INFLUENCE_BEATMAP`, `REMOVE_INFLUENCE_BEATMAP`, `EDIT_INFLUENCE_DESC`,
/// `EDIT_INFLUENCE_TYPE` activity types.
///
/// This is a placeholder type for documentation only. It's the same as `UserSmall`
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct UserSmallActivity {
    #[serde(flatten)]
    inner: UserSmall,
}

/// Added or removed beatmap. `OsuBeatmapSmall` type. For `ADD_USER_BEATMAP`, `REMOVE_USER_BEATMAP`,
/// `ADD_INFLUENCE_BEATMAP`, `REMOVE_INFLUENCE_BEATMAP` activity types.
///
/// This is a placeholder type for documentation only. It's the same as `OsuBeatmapSmall`
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuBeatmapSmallActivity {
    #[serde(flatten)]
    inner: OsuBeatmapSmall,
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
