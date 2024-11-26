use std::sync::Arc;

use async_trait::async_trait;

use surrealdb::{method::QueryStream, Notification};

use crate::{error::AppError, handlers::activity::Activity, retry::Retryable};

use super::{numerical_thing, DatabaseClient};

impl DatabaseClient {
    // Can't automate it in database
    // db has no way of differentiating login and influence add activities
    // we update the user when these two happens
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

    fn activity_query_string() -> &'static str {
        r#"
        SELECT 
            *,
            created_at,  
            meta::id(id) as id,
            event_type,
            
            meta::id(user.id) as user.id,
            user.username,
            user.avatar_url,
            user.country_code,
            user.country_name,
            user.groups,
            user.ranked_and_approved_beatmapset_count 
                + user.guest_beatmapset_count as user.ranked_maps,

            fn::id_or_null(influence.out.id) as influence.id,
            influence.out.username as influence.username,
            influence.out.avatar_url as influence.avatar_url,
            influence.out.country_code as influence.country_code,
            influence.out.country_name as influence.country_name,
            influence.out.groups as influence.groups,
            fn::add_possible_nulls(
                influence.out.ranked_and_approved_beatmapset_count, 
                influence.out.guest_beatmapset_count
            ) as influence.ranked_maps
            FROM activity
        "#
    }

    pub async fn get_activities(&self, limit: u32, start: u32) -> Result<Vec<Activity>, AppError> {
        let activities = self
            .db
            .query(format!(
                "{} {}",
                Self::activity_query_string(),
                "ORDER BY created_at DESC LIMIT $limit START $start"
            ))
            .bind(("limit", limit))
            .bind(("start", start))
            .await?
            .take(0)?;
        Ok(activities)
    }

    pub async fn start_activity_stream(
        &self,
    ) -> Result<QueryStream<Notification<Activity>>, AppError> {
        let mut response = self
            .db
            .query(format!("{} {}", "LIVE", Self::activity_query_string(),))
            .await?;
        let stream = response.stream::<Notification<Activity>>(0)?;
        Ok(stream)
    }
}

#[async_trait]
impl Retryable<QueryStream<Notification<Activity>>, AppError> for Arc<DatabaseClient> {
    async fn retry(&mut self) -> Result<QueryStream<Notification<Activity>>, AppError> {
        self.start_activity_stream().await
    }
}
