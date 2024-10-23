use serde::{Deserialize, Serialize};
use surrealdb::{sql::Datetime, RecordId};

use crate::error::AppError;

use super::{numerical_thing, DatabaseClient};

#[derive(Serialize, Deserialize, Debug)]
pub struct ActivityInfluence {
    out: RecordId,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ActivityCommonDbFields {
    id: RecordId,
    user: RecordId,
    created_at: Datetime,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "event_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DbActivity {
    Login {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
    },
    AddInfluence {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        influence: ActivityInfluence,
    },
    RemoveInfluence {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        influence: ActivityInfluence,
    },
    AddInfluenceBeatmap {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        beatmap: u32,
    },
    RemoveInfluenceBeatmap {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        beatmap: u32,
    },
    AddUserBeatmap {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        beatmap: u32,
    },
    RemoveUserBeatmap {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        beatmap: u32,
    },
    EditInfluenceDesc {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        description: String,
    },
    EditInfluenceType {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        influence_type: u8,
    },
    EditBio {
        #[serde(flatten)]
        common: ActivityCommonDbFields,
        bio: String,
    },
}

impl DatabaseClient {
    pub async fn add_login_activity(&self, user_id: u32) -> Result<(), AppError> {
        self.db
            .query(
                r#"
                CREATE activity 
                SET user = $user, 
                    created_at = time::now(), 
                    event_type = "LOGIN" 
                "#,
            )
            .bind(("user", numerical_thing("user", user_id)))
            .await?;
        Ok(())
    }

    pub async fn get_activities(
        &self,
        limit: u32,
        start: u32,
    ) -> Result<Vec<DbActivity>, AppError> {
        let activities = self
            .db
            .query("SELECT * FROM activities LIMIT $limit START &start ")
            .bind(("limit", limit))
            .bind(("start", start))
            .await?
            .take(0)?;
        Ok(activities)
    }
}
