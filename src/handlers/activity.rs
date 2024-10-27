use std::{collections::VecDeque, net::SocketAddr, sync::Arc};

use axum::{
    debug_handler,
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::Response,
    Json,
};
use futures::{SinkExt, StreamExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use surrealdb::sql::Datetime;
use tokio::sync::{
    broadcast::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    database::{user::UserSmall, DatabaseClient},
    error::AppError,
    osu_api::{
        BeatmapEnum, CachedRequester, CredentialsGrantClient, OsuBeatmapSmall, OsuMultipleBeatmap,
        OsuMultipleUser,
    },
    AppState,
};

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct ActivityCommonFields {
    id: String,
    user: UserSmall,
    #[schemars(with = "chrono::DateTime<chrono::Utc>")]
    created_at: Datetime,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
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

    pub fn get_current_queue(&self) -> Vec<Activity> {
        self.activity_queue.iter().cloned().collect()
    }

    pub fn get_activity_queue_string(&self) -> Result<String, AppError> {
        let string = serde_json::to_string(&self.activity_queue)?;
        Ok(string)
    }

    pub fn new_connection(&self) -> Result<(String, Receiver<String>), AppError> {
        Ok((
            self.get_activity_queue_string()?,
            self.activity_broadcaster.subscribe(),
        ))
    }

    pub fn spam_prevention(&self, _new_activity: &Activity) -> bool {
        true
    }

    pub async fn set_initial_activities(&mut self, db: &DatabaseClient) -> Result<(), AppError> {
        let step_size: usize = 100;
        'outer: for index in (0..).step_by(step_size) {
            let activity_chunk = db.get_activities(step_size as u32, index).await?;
            let activity_chunk_len = activity_chunk.len();
            for activity in activity_chunk {
                if self.spam_prevention(&activity) {
                    self.activity_queue.push_back(activity)
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

        let beatmaps = self
            .beatmap_requester
            .clone()
            .get_multiple_osu(&beatmaps_to_request, &token)
            .await?;
        let users_to_request: Vec<u32> = beatmaps.values().map(|beatmap| beatmap.user_id).collect();
        let users = self
            .user_requester
            .clone()
            .get_multiple_osu(&users_to_request, &token)
            .await?;
        self.activity_queue
            .iter_mut()
            .filter_map(|activity| {
                let id = activity.get_beatmap_id()?;
                // TODO: proper error handling plx
                let beatmap = beatmaps.get(&id)?;
                let user = users.get(&beatmap.user_id)?;
                Some((activity, beatmap, user))
            })
            .for_each(|(activity, beatmap, user)| {
                let beatmap_small = OsuBeatmapSmall::from_osu_beatmap_and_user_data(
                    beatmap.clone(),
                    user.username.clone(),
                    user.avatar_url.clone(),
                );
                activity.swap_beatmap_enum(BeatmapEnum::All(beatmap_small));
            });
        Ok(())
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Response, AppError> {
    let (initial_message, broadcast_receiver) = state.activity_tracker.new_connection()?;
    let upgrade_response = ws
        .on_upgrade(move |socket| handle_socket(socket, addr, initial_message, broadcast_receiver));
    Ok(upgrade_response)
}

// I hope we don't have to manually handle pings. Axum documentation claims that it's done
// automatically in background. But in my latest project, I had to do it manually since client
// library was sending ping messages in text format instead of its dedicated message type
// maybe that's how it's supposed to be? I don't think so but whatever
async fn handle_socket(
    websocket: WebSocket,
    address: SocketAddr,
    initial_data: String,
    mut broadcast_receiver: Receiver<String>,
) {
    let (ws_sender, mut ws_receiver) = websocket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    {
        let mut locked_ws_sender = ws_sender.lock().await;
        if let Err(error) = locked_ws_sender.send(Message::Text(initial_data)).await {
            tracing::error!(
                "Error while sending initial message to {}: {}",
                address,
                error
            );
            return;
        }
    }
    let ws_sender_clone = Arc::clone(&ws_sender);

    let websocket_task = tokio::spawn(async move {
        loop {
            match ws_receiver.next().await {
                Some(Ok(_)) => {
                    // Handle incoming WebSocket messages if needed
                }
                Some(Err(error)) => {
                    tracing::error!(
                        "Error while reading from websocket for {}: {}",
                        address,
                        error
                    );
                    break;
                }
                None => {
                    tracing::info!("WebSocket connection closed for {}", address);
                    break;
                }
            }
        }
    });

    let broadcast_task = tokio::spawn(async move {
        loop {
            match broadcast_receiver.recv().await {
                Ok(new_activity_string) => {
                    let mut locked_ws_sender = ws_sender_clone.lock().await;
                    if let Err(error) = locked_ws_sender
                        .send(Message::Text(new_activity_string))
                        .await
                    {
                        tracing::error!("Error while sending message to {}: {}", address, error);
                        break;
                    }
                }
                Err(error) => {
                    tracing::error!("Error receiving broadcast message: {}", error);
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = websocket_task => {},
        _ = broadcast_task => {},
    }
}

#[debug_handler]
pub async fn get_latest_activities(State(state): State<Arc<AppState>>) -> Json<Vec<Activity>> {
    let activities = state.activity_tracker.clone().get_current_queue();
    Json(activities)
}
