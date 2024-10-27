use std::{collections::VecDeque, net::SocketAddr, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, WebSocketUpgrade,
    },
    response::Response,
    Extension,
};
use serde::{Deserialize, Serialize};
use surrealdb::sql::Datetime;
use tokio::sync::broadcast::{self, Receiver, Sender};

use crate::{
    database::{user::UserSmall, DatabaseClient},
    error::AppError,
    osu_api::{
        BeatmapEnum, CachedRequester, CredentialsGrantClient, OsuBeatmapSmall, OsuMultipleBeatmap,
        OsuMultipleUser,
    },
};

#[derive(Serialize, Deserialize, Debug)]
pub struct ActivityCommonFields {
    id: String,
    user: UserSmall,
    created_at: Datetime,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "event_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Activity {
    Login {
        #[serde(flatten)]
        common: ActivityCommonFields,
    },
    AddInfluence {
        #[serde(flatten)]
        common: ActivityCommonFields,
        influence: UserSmall,
    },
    RemoveInfluence {
        #[serde(flatten)]
        common: ActivityCommonFields,
        influence: UserSmall,
    },
    AddInfluenceBeatmap {
        #[serde(flatten)]
        common: ActivityCommonFields,
        beatmap: BeatmapEnum,
    },
    RemoveInfluenceBeatmap {
        #[serde(flatten)]
        common: ActivityCommonFields,
        beatmap: BeatmapEnum,
    },
    AddUserBeatmap {
        #[serde(flatten)]
        common: ActivityCommonFields,
        beatmap: BeatmapEnum,
    },
    RemoveUserBeatmap {
        #[serde(flatten)]
        common: ActivityCommonFields,
        beatmap: BeatmapEnum,
    },
    EditInfluenceDesc {
        #[serde(flatten)]
        common: ActivityCommonFields,
        description: String,
    },
    EditInfluenceType {
        #[serde(flatten)]
        common: ActivityCommonFields,
        influence_type: u8,
    },
    EditBio {
        #[serde(flatten)]
        common: ActivityCommonFields,
        bio: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActivityGroup {
    UserBeatmap,
    InfluenceAdd,
    InfluenceRemove,
    InfluenceEdit,
    InfluenceBeatmap,
    Bio,
    Other,
}

impl Activity {
    pub fn group(&self) -> ActivityGroup {
        match self {
            Activity::Login { .. } => ActivityGroup::Other,
            Activity::EditBio { .. } => ActivityGroup::Bio,
            Activity::AddUserBeatmap { .. } => ActivityGroup::UserBeatmap,
            Activity::RemoveUserBeatmap { .. } => ActivityGroup::UserBeatmap,
            Activity::AddInfluence { .. } => ActivityGroup::InfluenceAdd,
            Activity::RemoveInfluence { .. } => ActivityGroup::InfluenceRemove,
            Activity::EditInfluenceDesc { .. } => ActivityGroup::InfluenceEdit,
            Activity::EditInfluenceType { .. } => ActivityGroup::InfluenceEdit,
            Activity::AddInfluenceBeatmap { .. } => ActivityGroup::InfluenceBeatmap,
            Activity::RemoveInfluenceBeatmap { .. } => ActivityGroup::InfluenceBeatmap,
        }
    }

    pub fn get_beatmap_id(&self) -> Option<u32> {
        let beatmap_enum = match self {
            Activity::AddInfluenceBeatmap { beatmap, .. }
            | Activity::RemoveInfluenceBeatmap { beatmap, .. }
            | Activity::AddUserBeatmap { beatmap, .. }
            | Activity::RemoveUserBeatmap { beatmap, .. } => Some(beatmap),
            _ => None,
        }?;
        match beatmap_enum {
            BeatmapEnum::Id(id) => Some(*id),
            BeatmapEnum::All(_) => None,
        }
    }

    pub fn swap_beatmap_enum(&mut self, beatmap_with_data: BeatmapEnum) {
        match self {
            Activity::AddInfluenceBeatmap {
                ref mut beatmap, ..
            }
            | Activity::RemoveInfluenceBeatmap {
                ref mut beatmap, ..
            }
            | Activity::AddUserBeatmap {
                ref mut beatmap, ..
            }
            | Activity::RemoveUserBeatmap {
                ref mut beatmap, ..
            } => *beatmap = beatmap_with_data,
            _ => {}
        }
    }
}

pub struct ActivityTracker {
    activity_queue: VecDeque<Activity>,
    queue_size: u8,
    activity_broadcaster: Sender<String>,
    user_requester: Arc<CachedRequester<OsuMultipleUser>>,
    beatmap_requester: Arc<CachedRequester<OsuMultipleBeatmap>>,
    credentials_grant_client: Arc<CredentialsGrantClient>,
}

impl ActivityTracker {
    pub async fn new(
        db: &DatabaseClient,
        queue_size: u8,
        user_requester: Arc<CachedRequester<OsuMultipleUser>>,
        beatmap_requester: Arc<CachedRequester<OsuMultipleBeatmap>>,
        credentials_grant_client: Arc<CredentialsGrantClient>,
    ) -> Result<ActivityTracker, AppError> {
        let (broadcast_sender, _broadcast_receiver) = broadcast::channel(50);
        let mut activity_tracker = ActivityTracker {
            activity_queue: VecDeque::new(),
            queue_size,
            activity_broadcaster: broadcast_sender,
            user_requester,
            beatmap_requester,
            credentials_grant_client,
        };
        activity_tracker.set_initial_activities(db).await?;
        activity_tracker.swap_beatmaps().await?;
        Ok(activity_tracker)
    }

    pub fn new_connection(&self) -> Result<(String, Receiver<String>), AppError> {
        Ok((
            serde_json::to_string(&self.activity_queue)?,
            self.activity_broadcaster.subscribe(),
        ))
    }

    pub fn spam_prevention(&self, new_activity: &Activity) -> bool {
        true
    }

    pub async fn set_initial_activities(&mut self, db: &DatabaseClient) -> Result<(), AppError> {
        let step_size: usize = 100;
        'outer: for index in (0..).step_by(step_size) {
            let activity_chunk = db
                .get_activities(step_size as u32, index + step_size as u32)
                .await?;
            let activity_chunk_len = activity_chunk.len();
            for activity in activity_chunk {
                if self.spam_prevention(&activity) {
                    self.activity_queue.push_front(activity)
                }
                if self.activity_queue.len() >= self.queue_size.into() {
                    break 'outer;
                }
            }
            // there might not be enough activities to fill the queue
            // if that's the case, the outer for loop would turn into an infinite loop
            if activity_chunk_len < step_size {
                break;
            }
        }
        Ok(())
    }

    pub async fn swap_beatmaps(&mut self) -> Result<(), AppError> {
        let beatmaps_to_request: Vec<u32> = self
            .activity_queue
            .iter()
            .filter_map(|activity| activity.get_beatmap_id())
            .collect();

        let token = self.credentials_grant_client.get_access_token()?;

        let mut beatmaps = self
            .beatmap_requester
            .clone()
            .get_multiple_osu(&beatmaps_to_request, &token)
            .await?;

        let users_to_request: Vec<u32> = beatmaps.values().map(|beatmap| beatmap.user_id).collect();
        let mut users = self
            .user_requester
            .clone()
            .get_multiple_osu(&users_to_request, &token)
            .await?;

        self.activity_queue
            .iter_mut()
            .filter_map(|activity| {
                let id = activity.get_beatmap_id()?;
                // TODO: proper error handling plx
                let beatmap = beatmaps.remove(&id)?;
                let user = users.remove(&beatmap.user_id)?;
                Some((activity, beatmap, user))
            })
            .for_each(|(activity, beatmap, user)| {
                let beatmap_small = OsuBeatmapSmall::from_osu_beatmap_and_user_data(
                    beatmap,
                    user.username,
                    user.avatar_url,
                );
                activity.swap_beatmap_enum(BeatmapEnum::All(beatmap_small));
            });
        Ok(())
    }
    pub fn test(&self) {
        dbg!(&self.activity_queue);
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(activity_tracker): Extension<ActivityTracker>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Response, AppError> {
    let (initial_message, broadcast_receiver) = activity_tracker.new_connection()?;
    let upgrade_response = ws
        .on_upgrade(move |socket| handle_socket(socket, addr, initial_message, broadcast_receiver));
    Ok(upgrade_response)
}

// I hope we don't have to manually handle pings. Axum documentation claims that it's done
// automatically in background. But in my latest project, I had to do it manually since client
// library was sending ping messages in text format instead of its dedicated message type
// maybe that's how it's supposed to be? I don't think so but whatever
async fn handle_socket(
    mut websocket: WebSocket,
    address: SocketAddr,
    initial_data: String,
    mut broadcast_receiver: Receiver<String>,
) {
    if let Err(error) = websocket.send(Message::Text(initial_data)).await {
        tracing::error!("Error while sending message to {}: {}", address, error);
    }

    while let Ok(new_activity_string) = broadcast_receiver.recv().await {
        if let Err(error) = websocket.send(Message::Text(new_activity_string)).await {
            tracing::error!("Error while sending message to {}: {}", address, error);
        } else {
            break;
        }
    }
}
