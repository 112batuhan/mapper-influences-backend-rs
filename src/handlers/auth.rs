use std::sync::{Arc, LazyLock};

use axum::{
    extract::{Query, Request, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use reqwest::header::SET_COOKIE;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{error::AppError, AppState};

static POST_LOGIN_REDIRECT_URI: LazyLock<String> = LazyLock::new(|| {
    std::env::var("POST_LOGIN_REDIRECT_URI")
        .expect("Missing POST_LOGIN_REDIRECT_URI environment variable")
});

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuthQuery {
    code: String,
}

pub async fn osu_oauth2_redirect(
    Query(query_parameters): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let auth_response = state
        .request
        .get_osu_auth_token(query_parameters.code)
        .await?;
    let osu_user = state
        .request
        .get_token_user(&auth_response.access_token)
        .await?;

    let token = state.jwt.create_jwt(
        osu_user.id,
        osu_user.username.clone(),
        auth_response.access_token,
        auth_response.expires_in,
    )?;
    let mut redirect_response = Redirect::to(POST_LOGIN_REDIRECT_URI.as_str()).into_response();
    redirect_response.headers_mut().insert(
        SET_COOKIE,
        format!(
            "user_token:{}; HttpOnly; Max-Age=86400; Path=/; SameSite=lax",
            token
        )
        .parse()
        .unwrap(),
    );
    state.db.upsert_user(osu_user, true).await?;
    Ok(redirect_response)
}

pub async fn check_jwt_token(
    State(state): State<Arc<AppState>>,
    cookie_jar: CookieJar,
    mut request: Request,
    next: axum::middleware::Next,
) -> Result<Response, AppError> {
    let token = cookie_jar
        .get("user_token")
        .ok_or(AppError::MissingTokenCookie)?
        .value();
    let claims = state
        .jwt
        .verify_jwt(token)
        .map_err(|_| AppError::JwtVerification)?;

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}
