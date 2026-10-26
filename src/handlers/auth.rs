use std::sync::{Arc, LazyLock};

use aide::transform::TransformOperation;
use argon2::{
    password_hash::{
        rand_core::{OsRng, RngCore},
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};
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

use crate::{error::AppError, osu_api::authorize_url, AppState};

static POST_LOGIN_REDIRECT_URI: LazyLock<String> = LazyLock::new(|| {
    std::env::var("POST_LOGIN_REDIRECT_URI")
        .expect("Missing POST_LOGIN_REDIRECT_URI environment variable")
});

static ADMIN_PASSWORD_SALT: LazyLock<SaltString> =
    LazyLock::new(|| SaltString::generate(&mut OsRng));

static ADMIN_PASSWORD: LazyLock<PasswordHash> = LazyLock::new(|| {
    let admin_password_string =
        std::env::var("ADMIN_PASSWORD").expect("Missing ADMIN_PASSWORD environment variable");
    Argon2::default()
        .hash_password(admin_password_string.as_bytes(), &*ADMIN_PASSWORD_SALT)
        .expect("Hashing failed")
});

/// To make local development easier, we set this flag in environment variables to set some cookie
/// attributes dynamically
static DEPLOY_COOKIE: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("DEPLOY_COOKIE").is_ok_and(|value| value.to_lowercase() == "true")
});

/// Holds the random `state` between [`osu_oauth2_login`] and [`osu_oauth2_redirect`]
const OAUTH_STATE_COOKIE: &str = "oauth_state";

#[derive(Deserialize, JsonSchema)]
pub struct AuthQuery {
    code: String,
    state: String,
}

fn oauth_state_cookie(value: &str, max_age: i32) -> String {
    let mut cookie =
        format!("{OAUTH_STATE_COOKIE}={value};HttpOnly;Max-Age={max_age};Path=/oauth;SameSite=lax");
    if *DEPLOY_COOKIE {
        cookie += ";Secure";
    }
    cookie
}

/// The cookie is only ever set by [`osu_oauth2_login`], so a missing or empty one never matches
fn state_matches(cookie_state: Option<&str>, query_state: &str) -> bool {
    cookie_state.is_some_and(|cookie_state| !cookie_state.is_empty() && cookie_state == query_state)
}

/// Starts the osu! login. The same random `state` goes into a cookie and into the osu! url, and
/// [`osu_oauth2_redirect`] only accepts the login when the two match. Without this, another site
/// could send a user to the redirect with the attacker's own code and log them into the
/// attacker's account (login CSRF).
pub async fn osu_oauth2_login() -> Response {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let oauth_state: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();

    let mut response = Redirect::to(&authorize_url(&oauth_state)).into_response();
    response.headers_mut().append(
        SET_COOKIE,
        oauth_state_cookie(&oauth_state, 600).parse().unwrap(),
    );
    response
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
    cookie_jar: CookieJar,
) -> Result<Response, AppError> {
    let cookie_state = cookie_jar
        .get(OAUTH_STATE_COOKIE)
        .map(|cookie| cookie.value());
    if !state_matches(cookie_state, &query_parameters.state) {
        return Err(AppError::OAuthState);
    }

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
    headers.append(
        SET_COOKIE,
        oauth_state_cookie("deleted", -1).parse().unwrap(),
    );

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
    let AdminLogin { password, id } = admin_login;
    // Argon2 verification is CPU-heavy; run it off the async runtime thread
    let password_correct = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .verify_password(password.as_bytes(), &ADMIN_PASSWORD)
            .is_ok()
    })
    .await?;
    if !password_correct {
        return Err(AppError::WrongAdminPassword);
    }

    let client_credential_token = state.credentials_grant_client.get_access_token().await?;
    let osu_user = state
        .request
        .get_user_osu(&client_credential_token, id)
        .await?;

    // Token can expire earlier than specified here. If that's the case, get a new one.
    state.jwt.create_jwt(
        osu_user.id,
        osu_user.username.clone(),
        client_credential_token,
        86400,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_must_match_the_cookie() {
        assert!(state_matches(Some("abc123"), "abc123"));
        assert!(!state_matches(Some("abc123"), "other"));
        assert!(!state_matches(None, "abc123"));
        assert!(!state_matches(Some(""), ""));
    }

    #[tokio::test]
    async fn login_sends_the_cookie_state_to_osu() {
        std::env::set_var("CLIENT_ID", "1");
        std::env::set_var("REDIRECT_URI", "http://localhost:8000/oauth/osu-redirect");

        let response = osu_oauth2_login().await;
        let location =
            reqwest::Url::parse(response.headers()[http::header::LOCATION].to_str().unwrap())
                .unwrap();
        let url_state = location
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1;
        let cookie = response.headers()[SET_COOKIE].to_str().unwrap();

        assert_eq!(url_state.len(), 64);
        assert!(cookie.starts_with(&format!("{OAUTH_STATE_COOKIE}={url_state};HttpOnly;")));
    }
}
