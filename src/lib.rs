use std::sync::Arc;

use aide::axum::routing::{delete_with, get_with, patch_with, post_with};
use aide::axum::ApiRouter;
use axum::middleware;
use axum::routing::any;
use database::leaderboard::{LeaderboardBeatmap, LeaderboardUser};
use database::DatabaseClient;
use handlers::activity::ActivityTracker;
use handlers::leaderboard::LeaderboardCache;
use jwt::JwtUtil;
use osu_api::{CombinedRequester, CredentialsGrantClient, RequestClient};

pub mod custom_cache;
pub mod database;
pub mod error;
pub mod handlers;
pub mod jwt;
pub mod osu_api;
pub mod retry;

pub struct AppState {
    pub db: Arc<DatabaseClient>,
    pub request: Arc<RequestClient>,
    pub jwt: JwtUtil,
    pub cached_combined_requester: Arc<CombinedRequester>,
    pub activity_tracker: Arc<ActivityTracker>,
    pub credentials_grant_client: Arc<CredentialsGrantClient>,
    pub user_leaderboard_cache: LeaderboardCache<(bool, Option<String>), LeaderboardUser>,
    pub beatmap_leaderboard_cache: LeaderboardCache<bool, LeaderboardBeatmap>,
}

impl AppState {
    pub async fn new(
        request: Arc<RequestClient>,
        credentials_grant_client: Arc<CredentialsGrantClient>,
    ) -> AppState {
        let db = Arc::new(
            DatabaseClient::new()
                .await
                .expect("failed to initialize db connection"),
        );

        let cached_combined_requester =
            CombinedRequester::new(request.clone(), "https://osu.ppy.sh");

        let activity_tracker = ActivityTracker::new(
            db.clone(),
            50,
            cached_combined_requester.clone(),
            credentials_grant_client.clone(),
        )
        .await
        // TODO: better handle errors
        .expect("failed to initialize activity tracker");

        AppState {
            db,
            request: request.clone(),
            jwt: JwtUtil::new_jwt(),
            cached_combined_requester,
            activity_tracker,
            credentials_grant_client,
            user_leaderboard_cache: LeaderboardCache::new(300),
            beatmap_leaderboard_cache: LeaderboardCache::new(300),
        }
    }
}

pub fn routes(state: Arc<AppState>) -> ApiRouter<Arc<AppState>> {
    ApiRouter::new()
        .api_route(
            "/search/map",
            get_with(handlers::osu_search::osu_beatmap_search, |op| {
                op.tag("Search")
            }),
        )
        .api_route(
            "/search/user/:query",
            get_with(handlers::osu_search::osu_user_search, |op| op.tag("Search")),
        )
        .api_route(
            "/influence/:influenced_to",
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
            "/influence/:influenced_to/description",
            patch_with(handlers::influence::update_influence_description, |op| {
                op.tag("Influence")
            }),
        )
        .api_route(
            "/influence/:influenced_to/type/:type_id",
            patch_with(handlers::influence::update_influence_type, |op| {
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
            "/activity",
            get_with(handlers::activity::get_latest_activities, |op| {
                op.tag("Activity")
            }),
        )
        .route("/ws", any(handlers::activity::ws_handler))
        .api_route(
            "/oauth/osu-redirect",
            get_with(handlers::auth::osu_oauth2_redirect, |op| op.tag("Auth")),
        )
        .api_route(
            "/oauth/logout",
            get_with(handlers::auth::logout, |op| op.tag("Auth")),
        )
        .api_route(
            "/oauth/admin",
            post_with(handlers::auth::admin_login, |op| op.tag("Auth")),
        )
        .api_route(
            "/leaderboard/user",
            get_with(handlers::leaderboard::get_user_leaderboard, |op| {
                op.tag("Leaderboard")
            }),
        )
        .api_route(
            "/leaderboard/beatmap",
            get_with(handlers::leaderboard::get_beatmap_leaderboard, |op| {
                op.tag("Leaderboard")
            }),
        )
}
