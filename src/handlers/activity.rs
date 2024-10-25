use std::{collections::VecDeque, net::SocketAddr};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, WebSocketUpgrade,
    },
    response::Response,
    Extension,
};
use serde::{Deserialize, Serialize};
use surrealdb::{sql::Datetime, RecordId};
use tokio::sync::broadcast::{self, Receiver, Sender};

use crate::{database::user::UserCondensed, error::AppError, osu_api::BeatmapEnum};

#[derive(Serialize, Deserialize, Debug)]
pub struct ActivityCommonFields {
    id: String,
    user: UserCondensed,
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
        influence: UserCondensed,
    },
    RemoveInfluence {
        #[serde(flatten)]
        common: ActivityCommonFields,
        influence: UserCondensed,
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
}

pub struct ActivityTracker {
    data_queue: VecDeque<Activity>,
    queue_size: u8,
    activity_broadcaster: Sender<String>,
}

impl ActivityTracker {
    async fn new(queue_size: u8) -> ActivityTracker {
        let (broadcast_sender, _broadcast_receiver) = broadcast::channel(50);
        ActivityTracker {
            data_queue: VecDeque::new(),
            queue_size,
            activity_broadcaster: broadcast_sender,
        }
    }

    fn new_connection(&self) -> Result<(String, Receiver<String>), AppError> {
        Ok((
            serde_json::to_string(&self.data_queue)?,
            self.activity_broadcaster.subscribe(),
        ))
    }

    fn spam_prevention(&self, new_activity: Activity) -> bool {
        true
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
