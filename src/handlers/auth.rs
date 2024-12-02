use std::sync::{Arc, LazyLock};

use aide::transform::TransformOperation;
use axum::{
    extract::{Query, Request, State},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use axum_extra::extract::CookieJar;
use futures::try_join;
use http::HeaderMap;
use reqwest::header::SET_COOKIE;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::AppError, AppState};

static POST_LOGIN_REDIRECT_URI: LazyLock<String> = LazyLock::new(|| {
    std::env::var("POST_LOGIN_REDIRECT_URI")
        .expect("Missing POST_LOGIN_REDIRECT_URI environment variable")
});
static ADMIN_PASSWORD: LazyLock<String> = LazyLock::new(|| {
    std::env::var("ADMIN_PASSWORD").expect("Missing ADMIN_PASSWORD environment variable")
});

/// To make local development easier, we set this flag in environment variables to set some cookie
/// attributes dynamically
static DEPLOY_COOKIE: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("DEPLOY_COOKIE").is_ok_and(|value| value.to_lowercase() == "true")
});

#[derive(Deserialize, JsonSchema)]
pub struct AuthQuery {
    code: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AdminLogin {
    password: String,
    /// Id of their osu account. This is so that they can act as their own account
    id: u32,
}

impl AdminLogin {
    pub fn new(password: String, id: u32) -> Self {
        Self { password, id }
    }
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
    let headers = redirect_response.headers_mut();
    let mut user_token_cookie_string = format!(
        "user_token={};HttpOnly;Max-Age=86400;Path=/;SameSite=lax",
        token
    );
    let mut logged_in_cookie_string =
        "logged_in=true;Max-Age=86400;Path=/;SameSite=lax".to_string();
    if *DEPLOY_COOKIE {
        user_token_cookie_string += ";Secure;domain=.mapperinfluences.com";
        logged_in_cookie_string += ";Secure;domain=.mapperinfluences.com";
    }

    headers.append(SET_COOKIE, user_token_cookie_string.parse().unwrap());
    headers.append(SET_COOKIE, logged_in_cookie_string.parse().unwrap());

    // TODO: maybe fix authorized thing to be in the same query later?
    let osu_user_id = osu_user.id;
    try_join!(
        state.db.add_login_activity(osu_user_id),
        state.db.upsert_user(osu_user)
    )?;
    state.db.set_authenticated(osu_user_id).await?;
    Ok(redirect_response)
}

pub fn osu_oauth2_redirect_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.tag("Auth").response::<302, ()>()
}

pub async fn logout() -> Response {
    let mut headers = HeaderMap::new();
    let mut user_token_cookie_string =
        "user_token=deleted;HttpOnly;Max-Age=-1;path=/;SameSite=lax".to_string();
    let mut logged_in_cookie_string = "logged_in=false;Max-Age=-1;path=/;SameSite=lax".to_string();
    if *DEPLOY_COOKIE {
        user_token_cookie_string += ";Secure;domain=.mapperinfluences.com";
        logged_in_cookie_string += ";Secure;domain=.mapperinfluences.com";
    }
    headers.append(SET_COOKIE, user_token_cookie_string.parse().unwrap());
    headers.append(SET_COOKIE, logged_in_cookie_string.parse().unwrap());
    headers.into_response()
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

/// Easy way to get a premade jwt with internal client credential grant method in it
///
/// This is to make the API testing easier by skipping oauth2 process
pub async fn admin_login(
    State(state): State<Arc<AppState>>,
    Json(admin_login): Json<AdminLogin>,
) -> Result<String, AppError> {
    if *ADMIN_PASSWORD != admin_login.password {
        return Err(AppError::WrongAdminPassword);
    }

    let client_credential_token = state.credentials_grant_client.get_access_token().await?;
    let osu_user = state
        .request
        .get_user_osu(&client_credential_token, admin_login.id)
        .await?;

    // Token can expire earlier than specified here. If that's the case, get a new one.
    state.jwt.create_jwt(
        osu_user.id,
        osu_user.username.clone(),
        client_credential_token,
        84600,
    )
}
