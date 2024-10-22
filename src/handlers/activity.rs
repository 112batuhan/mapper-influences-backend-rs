use std::{collections::VecDeque, net::SocketAddr};

use axum::{
    extract::{ws::WebSocket, ConnectInfo, WebSocketUpgrade},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{self, Receiver, Sender};

use crate::{
    database::{influence::InfluenceWithoutBeatmaps, user::UserCondensed},
    osu_api::OsuBeatmapCondensed,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActivityType {
    EditBio,
    AddBeatmap,
    RemoveBeatmap,
    AddInfluence,
    RemoveInfluence,
    EditInfluence,
    AddInfluenceBeatmap,
    RemoveInfluenceBeatmap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActivityGroup {
    Beatmap,
    InfluenceAdd,
    InfluenceRemove,
    InfluenceEdit,
    InfluenceBeatmap,
    Bio,
}

// Implement a method on ActivityType to return the corresponding ActivityGroup
impl ActivityType {
    pub fn group(&self) -> ActivityGroup {
        match self {
            ActivityType::EditBio => ActivityGroup::Bio,
            ActivityType::AddBeatmap => ActivityGroup::Beatmap,
            ActivityType::RemoveBeatmap => ActivityGroup::Beatmap,
            ActivityType::AddInfluence => ActivityGroup::InfluenceAdd,
            ActivityType::RemoveInfluence => ActivityGroup::InfluenceRemove,
            ActivityType::EditInfluence => ActivityGroup::InfluenceEdit,
            ActivityType::AddInfluenceBeatmap => ActivityGroup::InfluenceBeatmap,
            ActivityType::RemoveInfluenceBeatmap => ActivityGroup::InfluenceBeatmap,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityDetails {
    pub influenced_to: Option<InfluenceWithoutBeatmaps>,
    pub beatmap: Option<OsuBeatmapCondensed>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Activity {
    #[serde(rename = "type")]
    pub activity_type: ActivityType,
    pub user: UserCondensed,
    pub datetime: std::time::SystemTime,
    pub details: ActivityDetails,
}
pub struct ActivityTracker {
    data_queue: VecDeque<Activity>,
    queue_size: u8,
    activity_broadcaster: Sender<Activity>,
}

impl ActivityTracker {
    async fn new(queue_size: u8) -> ActivityTracker {
        let (broadcast_sender, _broadcast_receiver) = broadcast::channel(10);
        ActivityTracker {
            data_queue: VecDeque::new(),
            queue_size,
            activity_broadcaster: broadcast_sender,
        }
    }

    async fn new_connection(&self) -> (Vec<Activity>, Receiver<Activity>) {
        (
            self.data_queue.iter().cloned().collect::<Vec<Activity>>(),
            self.activity_broadcaster.subscribe(),
        )
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, addr))
}

async fn handle_socket(mut socket: WebSocket, who: SocketAddr) {}
