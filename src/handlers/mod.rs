use std::{collections::HashSet, sync::Arc};

use itertools::Itertools;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    error::AppError,
    osu_api::{cached_requester::CombinedRequester, BeatmapEnum, GetID},
};

pub mod activity;
pub mod auth;
pub mod graph_vizualizer;
pub mod influence;
pub mod leaderboard;
pub mod osu_search;
pub mod user;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PaginationQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    start: u32,
}
fn default_limit() -> u32 {
    100
}

// TODO: good opportunity to try macros?
// needed for aide documentation
#[derive(Deserialize, JsonSchema)]
pub struct PathInfluencedTo {
    #[serde(rename = "influenced_to")]
    value: u32,
}

#[derive(Deserialize, JsonSchema)]
pub struct PathBeatmapId {
    #[serde(rename = "beatmap_id")]
    value: u32,
}

#[derive(Deserialize, JsonSchema)]
pub struct PathUserId {
    #[serde(rename = "user_id")]
    value: u32,
}

#[derive(Deserialize, JsonSchema)]
pub struct PathQuery {
    #[serde(rename = "query")]
    pub value: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct PathUserBeatmapIds {
    pub influenced_to: u32,
    pub beatmap_id: u32,
}
#[derive(Deserialize, JsonSchema)]
pub struct PathUserTypeId {
    pub influenced_to: u32,
    pub type_id: u8,
}

/// `BeatmapRequest` type
#[derive(Deserialize, JsonSchema)]
pub struct BeatmapRequest {
    #[serde(rename = "beatmaps")]
    ids: HashSet<u32>,
}

/// A shortcut to use in user and influence endpoints.
/// This is not usable for multiple influences as this function would send requests for each
/// influence. They have their own implementation to save requests
///
/// TODO: maybe even do it as middleware? you seem to repeat this. A little ambitious though
async fn swap_beatmaps(
    cached_combined_requester: Arc<CombinedRequester>,
    osu_token: &str,
    beatmaps: &mut Vec<BeatmapEnum>,
) -> Result<(), AppError> {
    let beatmaps_to_request: Vec<u32> = beatmaps.iter().map(|map| map.get_id()).unique().collect();

    let mut requested_beatmaps = cached_combined_requester
        .clone()
        .get_beatmaps_with_user(&beatmaps_to_request, osu_token)
        .await?;

    // to keep the order, we iterate user beatmaps
    let new_beatmaps: Vec<BeatmapEnum> = beatmaps
        .iter()
        .filter_map(|beatmap_enum| {
            // remove should be ok, we keep beatmaps as set in db, so they should be unique
            let beatmap = requested_beatmaps.remove(&beatmap_enum.get_id())?;
            Some(BeatmapEnum::All(beatmap))
        })
        .collect();

    *beatmaps = new_beatmaps;
    Ok(())
}

async fn check_multiple_maps(
    cached_combined_requester: Arc<CombinedRequester>,
    osu_token: &str,
    beatmaps: &[u32],
) -> Result<(), AppError> {
    let requested_beatmaps = cached_combined_requester
        .clone()
        .get_beatmaps_only(beatmaps, osu_token)
        .await?;

    // efficient but not user friendly missing map warning
    let first_missing_beatmap = requested_beatmaps
        .keys()
        .filter(|requested_map| !beatmaps.contains(requested_map))
        .copied()
        .next();
    if let Some(first_missing_map) = first_missing_beatmap {
        return Err(AppError::NonExistingMap(first_missing_map));
    }
    Ok(())
}
