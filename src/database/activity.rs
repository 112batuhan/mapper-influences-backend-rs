use crate::error::AppError;

use super::{numerical_thing, DatabaseClient};

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
}
