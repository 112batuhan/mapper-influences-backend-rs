use serde::{Deserialize, Serialize};
use surrealdb::sql::{Datetime, Thing};

use crate::error::AppError;

use super::{numerical_thing, DatabaseClient};

#[derive(Serialize, Deserialize)]
pub struct ActivityInfluence {
    out: Thing,
}

#[derive(Serialize, Deserialize)]
pub struct ActivityCommonDbFields {
    id: Thing,
    user: Thing,
    created_at: Datetime,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "SCREAMING_SNAKE_CASE")]
enum DbActivity {
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

    pub async fn get_activities(&self, limit: u32, start: u32) -> Result<(), AppError> {
        self.db
            .query(
                r#"
                 Select * from activity 
                "#,
            )
            .await?;
        Ok(())
    }
}
