use jwt_simple::{
    algorithms::{HS256Key, MACLike},
    claims::Claims,
    reexports::coarsetime::Duration,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthData {
    pub osu_token: String,
    pub user_id: u32,
    pub username: String,
}

pub struct JwtUtil {
    pub key: HS256Key,
}
impl JwtUtil {
    pub fn new_jwt() -> JwtUtil {
        let key_str =
            std::env::var("JWT_SECRET_KEY").expect("JWT_SECRET_KEY env variable is not set");
        let key = HS256Key::from_bytes(key_str.as_bytes());

        JwtUtil { key }
    }

    pub fn create_jwt(
        &self,
        id: u32,
        username: String,
        osu_token: String,
        duration: u32,
    ) -> Result<String, AppError> {
        let additional_data = AuthData {
            osu_token,
            user_id: id,
            username,
        };
        let claims =
            Claims::with_custom_claims(additional_data, Duration::from_secs(duration.into()));
        let token = self.key.authenticate(claims)?;
        Ok(token)
    }

    pub fn verify_jwt(&self, token: &str) -> Result<AuthData, AppError> {
        let claims = self.key.verify_token::<AuthData>(token, None)?;
        Ok(claims.custom)
    }
}
