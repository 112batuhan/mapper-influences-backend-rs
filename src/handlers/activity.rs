use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{Arc, Mutex as StdMutex, MutexGuard},
};

use axum::{
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
use surrealdb::{method::QueryStream, sql::Datetime, Action, Notification};
use tokio::sync::{
    broadcast::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    database::{user::UserSmall, DatabaseClient},
    documentation,
    error::AppError,
    osu_api::{
        cached_requester::CombinedRequester, credentials_grant::CredentialsGrantClient,
        BeatmapEnum, GetID,
    },
    retry::Retryable,
    AppState,
};

/// `Activity` type
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct Activity {
    id: String,
    user: UserSmall,
    #[schemars(with = "chrono::DateTime<chrono::Utc>")]
    created_at: Datetime,
    #[schemars(with = "documentation::FlattenedActivityType")]
    #[serde(flatten)]
    activity_type: ActivityType,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(tag = "event_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActivityType {
    Login,
    AddInfluence {
        influence: UserSmall,
    },
    RemoveInfluence {
        influence: UserSmall,
    },
    AddUserBeatmap {
        beatmap: BeatmapEnum,
    },
    RemoveUserBeatmap {
        beatmap: BeatmapEnum,
    },
    AddInfluenceBeatmap {
        influence: UserSmall,
        beatmap: BeatmapEnum,
    },
    RemoveInfluenceBeatmap {
        influence: UserSmall,
        beatmap: BeatmapEnum,
    },
    EditInfluenceDesc {
        influence: UserSmall,
        description: String,
    },
    EditInfluenceType {
        influence: UserSmall,
        influence_type: u8,
    },
    EditBio {
        bio: String,
    },
}

impl ActivityType {
    pub fn get_beatmap_id(&self) -> Option<u32> {
        let beatmap_enum = match self {
            ActivityType::AddInfluenceBeatmap { beatmap, .. }
            | ActivityType::RemoveInfluenceBeatmap { beatmap, .. }
            | ActivityType::AddUserBeatmap { beatmap, .. }
            | ActivityType::RemoveUserBeatmap { beatmap, .. } => Some(beatmap),
            _ => None,
        }?;
        match beatmap_enum {
            BeatmapEnum::Id(id) => Some(*id),
            BeatmapEnum::All(_) => None,
        }
    }

    pub fn swap_beatmap_enum(&mut self, beatmap_with_data: BeatmapEnum) {
        match self {
            ActivityType::AddInfluenceBeatmap {
                ref mut beatmap, ..
            }
            | ActivityType::RemoveInfluenceBeatmap {
                ref mut beatmap, ..
            }
            | ActivityType::AddUserBeatmap {
                ref mut beatmap, ..
            }
            | ActivityType::RemoveUserBeatmap {
                ref mut beatmap, ..
            } => *beatmap = beatmap_with_data,
            _ => {}
        }
    }
}

pub struct ActivityTracker {
    activity_queue: StdMutex<VecDeque<Activity>>,
    queue_size: u8,
    activity_broadcaster: Sender<String>,
    cached_combined_requester: Arc<CombinedRequester>,
    credentials_grant_client: Arc<CredentialsGrantClient>,
}

impl ActivityTracker {
    pub async fn new(
        db: Arc<DatabaseClient>,
        queue_size: u8,
        cached_combined_requester: Arc<CombinedRequester>,
        credentials_grant_client: Arc<CredentialsGrantClient>,
    ) -> Result<Arc<ActivityTracker>, AppError> {
        let (broadcast_sender, _broadcast_receiver) = broadcast::channel(50);
        let activity_tracker = ActivityTracker {
            activity_queue: StdMutex::new(VecDeque::new()),
            queue_size,
            activity_broadcaster: broadcast_sender,
            cached_combined_requester,
            credentials_grant_client,
        };
        let activity_tracker = Arc::new(activity_tracker);
        activity_tracker.set_initial_activities(&db).await?;
        activity_tracker.swap_beatmaps().await?;
        activity_tracker.clone().start_loop(db).await?;
        Ok(activity_tracker)
    }

    pub fn lock_activity_queue(&self) -> Result<MutexGuard<VecDeque<Activity>>, AppError> {
        self.activity_queue.lock().map_err(|_| AppError::Mutex)
    }

    pub fn add_new_activity_to_queue(&self, new_activity: Activity) -> Result<(), AppError> {
        let mut locked_queue = self.lock_activity_queue()?;
        locked_queue.push_back(new_activity);
        if locked_queue.len() > self.queue_size.into() {
            locked_queue.pop_front();
        }
        Ok(())
    }

    pub fn get_current_queue(&self) -> Result<Vec<Activity>, AppError> {
        let cloned = { self.lock_activity_queue()?.iter().cloned().collect() };
        Ok(cloned)
    }

    pub fn new_connection(&self) -> Result<(String, Receiver<String>), AppError> {
        Ok((
            serde_json::to_string(&self.activity_queue)?,
            self.activity_broadcaster.subscribe(),
        ))
    }

    pub fn spam_prevention(&self, new_activity: &Activity) -> Result<bool, AppError> {
        let locked_queue = self.lock_activity_queue()?;

        match &new_activity.activity_type {
            ActivityType::EditBio { .. } => Ok(!locked_queue.iter().any(|old_activity| {
                new_activity.user.id == old_activity.user.id
                    && matches!(old_activity.activity_type, ActivityType::EditBio { .. })
            })),
            ActivityType::AddUserBeatmap {
                beatmap: new_beatmap,
            } => {
                let max_false = 1;
                let mut current_false = 0;
                let matched = locked_queue.iter().any(|old_activity| {
                    new_activity.user.id == old_activity.user.id
                        && match &old_activity.activity_type {
                            ActivityType::AddUserBeatmap {
                                beatmap: old_beatmap,
                            } => {
                                if new_beatmap.get_id() != old_beatmap.get_id()
                                    && current_false <= max_false
                                {
                                    current_false += 1;
                                    false
                                } else {
                                    true
                                }
                            }
                            _ => false,
                        }
                });
                Ok(!matched)
            }

            ActivityType::AddInfluence {
                influence: new_influence,
            } => {
                let matched = locked_queue.iter().any(|old_activity| {
                    new_activity.user.id == old_activity.user.id
                        && match &old_activity.activity_type {
                            ActivityType::AddInfluence {
                                influence: old_influence,
                            }
                            | ActivityType::EditInfluenceDesc {
                                influence: old_influence,
                                ..
                            }
                            | ActivityType::EditInfluenceType {
                                influence: old_influence,
                                ..
                            } => new_influence.id == old_influence.id,
                            _ => false,
                        }
                });
                Ok(!matched)
            }
            ActivityType::EditInfluenceDesc {
                influence: new_influence,
                ..
            }
            | ActivityType::EditInfluenceType {
                influence: new_influence,
                ..
            } => {
                let matched = locked_queue.iter().any(|old_activity| {
                    new_activity.user.id == old_activity.user.id
                        && match &old_activity.activity_type {
                            ActivityType::AddInfluence {
                                influence: old_influence,
                            }
                            | ActivityType::EditInfluenceDesc {
                                influence: old_influence,
                                ..
                            }
                            | ActivityType::EditInfluenceType {
                                influence: old_influence,
                                ..
                            } => new_influence.id == old_influence.id,

                            _ => false,
                        }
                });
                Ok(!matched)
            }
            ActivityType::AddInfluenceBeatmap {
                influence: new_influence,
                beatmap: new_beatmap,
            } => {
                let max_false = 1;
                let mut current_false = 0;
                let matched = locked_queue.iter().any(|old_activity| {
                    new_activity.user.id == old_activity.user.id
                        && match &old_activity.activity_type {
                            ActivityType::AddInfluenceBeatmap {
                                influence: old_influence,
                                beatmap: old_beatmap,
                            } => {
                                if new_influence.id != old_influence.id
                                    || new_beatmap.get_id() != old_beatmap.get_id()
                                        && current_false <= max_false
                                {
                                    current_false += 1;
                                    false
                                } else {
                                    true
                                }
                            }
                            _ => false,
                        }
                });
                Ok(!matched)
            }
            _ => Ok(false),
        }
    }

    pub async fn set_initial_activities(&self, db: &DatabaseClient) -> Result<(), AppError> {
        let step_size: usize = self.queue_size as usize * 2;
        'outer: for index in (0..).step_by(step_size) {
            let activity_chunk = db.get_activities(step_size as u32, index).await?;
            let activity_chunk_len = activity_chunk.len();
            for activity in activity_chunk {
                // unoptimized lock usage doesn't matter here.
                // This is only going to run at the start of the program once
                if self.spam_prevention(&activity)? {
                    self.lock_activity_queue()?.push_back(activity);
                }
                if self.lock_activity_queue()?.len() >= self.queue_size.into() {
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

    pub async fn swap_beatmaps(&self) -> Result<(), AppError> {
        let beatmaps_to_request: Vec<u32> = {
            self.lock_activity_queue()?
                .iter()
                .filter_map(|activity| activity.activity_type.get_beatmap_id())
                .collect()
        };

        if beatmaps_to_request.is_empty() {
            return Ok(());
        }

        let token = self.credentials_grant_client.get_access_token().await?;
        let beatmaps = self
            .cached_combined_requester
            .clone()
            .get_beatmaps_with_user(&beatmaps_to_request, &token)
            .await?;

        self.lock_activity_queue()?
            .iter_mut()
            .filter_map(|activity| {
                let id = activity.activity_type.get_beatmap_id()?;
                // it's not ok to use remove here
                // there could be beatmaps used more than once
                let beatmap = beatmaps.get(&id)?;
                Some((activity, beatmap))
            })
            .for_each(|(activity, beatmap)| {
                activity
                    .activity_type
                    .swap_beatmap_enum(BeatmapEnum::All(beatmap.clone()));
            });
        Ok(())
    }

    async fn start_loop(self: Arc<Self>, mut db: Arc<DatabaseClient>) -> Result<(), AppError> {
        let mut stream: QueryStream<Notification<Activity>> = db
            .retry_until_success(60, "Failed to start activity stream")
            .await;
        let broadcast_sender = self.activity_broadcaster.clone();
        let cloned_self = self.clone();
        tokio::spawn(async move {
            loop {
                // We can't return from this task
                // Best we can do is to attempt to retry if something goes wrong
                // This should mean that the rest of the backend is also not working

                let stream_result = match stream.next().await {
                    Some(stream_result) => stream_result,
                    None => {
                        stream = db
                            .retry_until_success(60, "Activity stream has been closed")
                            .await;
                        tracing::info!("Activity stream connected again.");
                        continue;
                    }
                };

                let mut new_activity = match stream_result {
                    Ok(new_action) => new_action,
                    Err(surrealdb::Error::Db(surrealdb::error::Db::Serialization(error))) => {
                        tracing::debug!(
                            "Serialization error. An activity record was manually deleted. \
                            Details: {}",
                            error
                        );
                        continue;
                    }
                    Err(error) => {
                        // I don't think we need to restart the activity stream here. But just in
                        // case. If it goes bad, I will remove it
                        let message =
                            format!("Unexpected error in activity stream thread: {}", error);
                        stream = db.retry_until_success(60, &message).await;
                        continue;
                    }
                };

                // Logging unexpected notification actions. This could be useful for debbugging
                // the errors that might occur with the stream especially for delete action. since
                // the surrealdb sends undeserializable data for that, so we have to manually skip
                // them in error handling. But that might not always be the case
                match &new_activity.action {
                    Action::Update => {
                        tracing::debug!(
                            "New activity update action with id: {}",
                            &new_activity.data.id
                        );
                        continue;
                    }
                    Action::Delete => {
                        tracing::debug!(
                            "New activity delete action with id: {}",
                            &new_activity.data.id
                        );
                        continue;
                    }
                    _ => {}
                }

                let Ok(true) = cloned_self.spam_prevention(&new_activity.data) else {
                    continue;
                };
                if let Some(beatmap_id) = &new_activity.data.activity_type.get_beatmap_id() {
                    let Ok(token) = cloned_self
                        .credentials_grant_client
                        .clone()
                        .get_access_token()
                        .await
                    else {
                        tracing::error!("RwLock error while trying to get access token");
                        continue;
                    };

                    let new_beatmap_map = match cloned_self
                        .cached_combined_requester
                        .get_beatmaps_with_user(&[*beatmap_id], &token)
                        .await
                    {
                        Ok(beatmap) => beatmap,
                        Err(error) => {
                            tracing::error!(
                                "Failed to request beatmap. Activity id: {}. Error: {}",
                                &new_activity.data.id,
                                error
                            );
                            continue;
                        }
                    };

                    let Some(new_beatmap) = new_beatmap_map.into_values().next() else {
                        tracing::error!(
                            "Failed to get beatmap. This should never happen! Activity id: {}",
                            &new_activity.data.id
                        );
                        continue;
                    };

                    new_activity
                        .data
                        .activity_type
                        .swap_beatmap_enum(BeatmapEnum::All(new_beatmap));
                };

                let Ok(activity_string) = serde_json::to_string(&new_activity.data) else {
                    tracing::error!(
                        "Failed to convert new activity object to json string. Activity id: {}",
                        &new_activity.data.id
                    );
                    continue;
                };

                if cloned_self
                    .add_new_activity_to_queue(new_activity.data)
                    .is_err()
                {
                    tracing::error!("Failed to add new activity to the queue");
                    continue;
                };

                if let Ok(receiver_count) = broadcast_sender.send(activity_string) {
                    tracing::info!("Sending new activity to {} connections", receiver_count);
                } else {
                    tracing::info!("There is no receiver for new activities");
                }
            }
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

pub async fn get_latest_activities(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Activity>>, AppError> {
    let activities = state.activity_tracker.get_current_queue()?;
    Ok(Json(activities))
}
