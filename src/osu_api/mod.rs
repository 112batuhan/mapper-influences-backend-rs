use std::sync::LazyLock;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod cached_requester;
pub mod credentials_grant;
pub mod request;

static CLIENT_ID: LazyLock<String> =
    LazyLock::new(|| std::env::var("CLIENT_ID").expect("Missing CLIENT_ID environment variable"));

static CLIENT_SECRET: LazyLock<String> = LazyLock::new(|| {
    std::env::var("CLIENT_SECRET").expect("Missing CLIENT_SECRET environment variable")
});

static REDIRECT_URI: LazyLock<String> = LazyLock::new(|| {
    std::env::var("REDIRECT_URI").expect("Missing REDIRECT_URI environment variable")
});

/// Also has `refresh_token` but we don't need it
#[derive(Serialize, Deserialize, Debug)]
pub struct OsuAuthToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u32,
}

#[derive(Serialize, Debug)]
pub struct AuthRequest {
    pub client_id: &'static str,
    pub client_secret: &'static str,
    pub grant_type: &'static str,
    pub redirect_uri: &'static str,
    pub scope: Option<&'static str>,
    pub code: Option<String>,
}

impl AuthRequest {
    fn authorization(code: String) -> AuthRequest {
        AuthRequest {
            client_id: &CLIENT_ID,
            client_secret: &CLIENT_SECRET,
            redirect_uri: &REDIRECT_URI,
            grant_type: "authorization_code",
            code: Some(code),
            scope: None,
        }
    }

    fn client_credential() -> AuthRequest {
        AuthRequest {
            client_id: &CLIENT_ID,
            client_secret: &CLIENT_SECRET,
            redirect_uri: &REDIRECT_URI,
            grant_type: "client_credentials",
            code: None,
            scope: Some("public"),
        }
    }
}

pub trait GetID {
    fn get_id(&self) -> u32;
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct UserId {
    pub id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuMultipleUser {
    pub id: u32,
    pub avatar_url: String,
    pub username: String,
}
impl GetID for OsuMultipleUser {
    fn get_id(&self) -> u32 {
        self.id
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct Country {
    pub code: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq, Eq)]
/// `Group` type
pub struct Group {
    pub colour: Option<String>,
    pub name: String,
    pub short_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct UserOsu {
    pub id: u32,
    pub username: String,
    pub avatar_url: String,
    pub country: Country,
    pub groups: Vec<Group>,
    pub previous_usernames: Vec<String>,
    pub ranked_and_approved_beatmapset_count: u32,
    pub ranked_beatmapset_count: u32,
    pub nominated_beatmapset_count: u32,
    pub guest_beatmapset_count: u32,
    pub loved_beatmapset_count: u32,
    pub graveyard_beatmapset_count: u32,
    pub pending_beatmapset_count: u32,
}
impl UserOsu {
    pub fn is_ranked_mapper(&self) -> bool {
        self.ranked_beatmapset_count + self.loved_beatmapset_count + self.guest_beatmapset_count > 0
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuSearchUserData {
    pub data: Vec<UserId>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuSearchUserResponse {
    pub user: OsuSearchUserData,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
/// `BeatmapOsu` type. Used in `SearchBeatmapset` type
pub struct BeatmapOsu {
    pub difficulty_rating: f64,
    pub id: u32,
    pub mode: String,
    pub beatmapset_id: u32,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct BeatmapsetRelatedUser {
    pub username: String,
    pub avatar_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct Cover {
    pub cover: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct BaseBeatmapset {
    pub beatmaps: Vec<BeatmapOsu>,
    pub title: String,
    pub artist: String,
    pub covers: Cover,
    pub creator: String,
    pub id: u32,
    pub user_id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct BeatmapsetOsu {
    #[serde(flatten)]
    pub base_beatmapset: BaseBeatmapset,
    pub related_users: Vec<BeatmapsetRelatedUser>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuSearchMapResponse {
    pub beatmapsets: Vec<BaseBeatmapset>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuMultipleBeatmap {
    pub id: u32,
    pub difficulty_rating: f32,
    pub mode: String,
    pub beatmapset_id: u32,
    pub version: String,
    pub user_id: u32,
    pub beatmapset: OsuMultipleBeatmapsetResponse,
}

impl GetID for OsuMultipleBeatmap {
    fn get_id(&self) -> u32 {
        self.id
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct OsuMultipleBeatmapsetResponse {
    pub title: String,
    pub artist: String,
    pub covers: Cover,
    pub user_id: u32,
    pub creator: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
/// `OsuBeatmapSmall` type. Mainly for beatmap cards
pub struct OsuBeatmapSmall {
    pub id: u32,
    pub difficulty_rating: f32,
    pub mode: String,
    pub beatmapset_id: u32,
    pub version: String,
    pub user_id: u32,
    pub user_name: String,
    pub user_avatar_url: String,
    pub title: String,
    pub artist: String,
    pub cover: String,
}

impl OsuBeatmapSmall {
    /// This function combines [`OsuMultipleBeatmap`] and [`OsuMultipleUser`].
    ///
    /// If user is not returned from the query, we fallback to beatmapset user.
    /// This usually happens if the original mapper is banned. If the beatmapset submitter is also
    /// banned, we don't have to worry about the avatar_url as osu automatically falls back to
    /// guest picture.
    pub fn from_osu_beatmap_and_user_data(
        osu_multiple: OsuMultipleBeatmap,
        user_multiple: Option<OsuMultipleUser>,
    ) -> OsuBeatmapSmall {
        let user_name: String;
        let user_avatar_url: String;

        if let Some(user_multiple) = user_multiple {
            user_name = user_multiple.username;
            user_avatar_url = user_multiple.avatar_url;
        } else {
            user_name = osu_multiple.beatmapset.creator;
            user_avatar_url = format!("https://a.ppy.sh/{}?", osu_multiple.beatmapset.user_id);
        }

        OsuBeatmapSmall {
            id: osu_multiple.id,
            difficulty_rating: osu_multiple.difficulty_rating,
            mode: osu_multiple.mode,
            beatmapset_id: osu_multiple.beatmapset_id,
            version: osu_multiple.version,
            user_id: osu_multiple.user_id,
            user_name,
            user_avatar_url,
            title: osu_multiple.beatmapset.title,
            artist: osu_multiple.beatmapset.artist,
            cover: osu_multiple.beatmapset.covers.cover,
        }
    }
}

/// Despite having two variants for beatmaps, the API will always return the full beatmap
/// objects instead of integer id's.
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, PartialEq)]
#[serde(untagged)]
pub enum BeatmapEnum {
    All(OsuBeatmapSmall),
    Id(u32),
}

impl GetID for BeatmapEnum {
    fn get_id(&self) -> u32 {
        match self {
            BeatmapEnum::All(beatmap) => beatmap.id,
            BeatmapEnum::Id(id) => *id,
        }
    }
}

impl GetID for &BeatmapEnum {
    fn get_id(&self) -> u32 {
        match self {
            BeatmapEnum::All(beatmap) => beatmap.id,
            BeatmapEnum::Id(id) => *id,
        }
    }
}
