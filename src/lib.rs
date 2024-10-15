use std::sync::Arc;

use aide::axum::routing::{delete_with, get_with, patch_with, post_with};
use aide::axum::ApiRouter;
use axum::middleware;
use database::DatabaseClient;
use jwt::JwtUtil;
use osu_api::RequestClient;

pub mod custom_cache;
pub mod database;
pub mod error;
pub mod handlers;
pub mod jwt;
pub mod osu_api;

pub struct AppState {
    pub db: DatabaseClient,
    pub request: RequestClient,
    pub jwt: JwtUtil,
}

impl AppState {
    pub async fn new() -> AppState {
        AppState {
            db: DatabaseClient::new()
                .await
                .expect("failed to initialize db connection"),
            request: RequestClient::new(10),
            jwt: JwtUtil::new_jwt(),
        }
    }
}

pub fn routes(state: Arc<AppState>) -> ApiRouter<Arc<AppState>> {
    ApiRouter::new()
        .api_route(
            "/osu-api/search/map",
            get_with(handlers::osu_api::osu_beatmap_search, |op| {
                op.tag("osu! API")
            }),
        )
        .api_route(
            "/osu-api/search/user/:query",
            get_with(handlers::osu_api::osu_user_search, |op| op.tag("osu! API")),
        )
        .api_route(
            "/osu-api/beatmap/:beatmap_id",
            get_with(handlers::osu_api::osu_beatmap, |op| op.tag("osu! API")),
        )
        .api_route(
            "/osu-api/user/:user_id",
            get_with(handlers::osu_api::osu_user, |op| op.tag("osu! API")),
        )
        .api_route(
            "/influence",
            post_with(handlers::influence::add_influence, |op| op.tag("Influence")),
        )
        .api_route(
            "/influence/influences/:user_id",
            get_with(handlers::influence::get_user_influences, |op| {
                op.tag("Influence")
            }),
        )
        .api_route(
            "/influence/mentions/:user_id",
            get_with(handlers::influence::get_user_mentions, |op| {
                op.tag("Influence")
            }),
        )
        .api_route(
            "/influence/:influenced_to",
            delete_with(handlers::influence::delete_influence, |op| {
                op.tag("Influence")
            }),
        )
        .api_route(
            "/influence/:influenced_to/map/:beatmap_id",
            patch_with(handlers::influence::add_influence_beatmap, |op| {
                op.tag("Influence")
            }),
        )
        .api_route(
            "/influence/:influenced_to/map/:beatmap_id",
            delete_with(handlers::influence::remove_influence_beatmap, |op| {
                op.tag("Influence")
            }),
        )
        .api_route(
            "/influence/:influenced_to/bio",
            patch_with(handlers::influence::update_influence_description, |op| {
                op.tag("Influence")
            }),
        )
        .api_route(
            "/users/me",
            get_with(handlers::user::get_me, |op| op.tag("User")),
        )
        .api_route(
            "/users/:user_id",
            get_with(handlers::user::get_user, |op| op.tag("User")),
        )
        .api_route(
            "/users/bio",
            patch_with(handlers::user::update_user_bio, |op| op.tag("User")),
        )
        .api_route(
            "/users/map/:beatmap_id",
            patch_with(handlers::user::add_user_beatmap, |op| op.tag("User")),
        )
        .api_route(
            "/users/map/:beatmap_id",
            delete_with(handlers::user::delete_user_beatmap, |op| op.tag("User")),
        )
        .api_route(
            "/user/influence-order",
            post_with(handlers::user::set_influence_order, |op| op.tag("User")),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            handlers::auth::check_jwt_token,
        ))
        .api_route(
            "/oauth/osu-redirect",
            get_with(handlers::auth::osu_oauth2_redirect, |op| op.tag("Auth")),
        )
        .api_route(
            "/leaderboard",
            get_with(handlers::leaderboard::get_leaderboard, |op| {
                op.tag("Leaderboard")
            }),
        )
}
