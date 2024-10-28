use std::sync::Arc;

use aide::axum::routing::{delete_with, get_with, patch_with, post_with};
use aide::axum::ApiRouter;
use axum::middleware;
use axum::routing::any;
use database::DatabaseClient;
use handlers::activity::ActivityTracker;
use jwt::JwtUtil;
use osu_api::{
    CachedRequester, CredentialsGrantClient, OsuMultipleBeatmap, OsuMultipleUser, RequestClient,
};

pub mod auto_update;
pub mod custom_cache;
pub mod database;
pub mod error;
pub mod handlers;
pub mod jwt;
pub mod osu_api;

pub struct AppState {
    pub db: Arc<DatabaseClient>,
    pub request: Arc<RequestClient>,
    pub jwt: JwtUtil,
    pub user_requester: Arc<CachedRequester<OsuMultipleUser>>,
    pub beatmap_requester: Arc<CachedRequester<OsuMultipleBeatmap>>,
    pub activity_tracker: Arc<ActivityTracker>,
}

impl AppState {
    pub async fn new(
        request: Arc<RequestClient>,
        credentials_grant_client: Arc<CredentialsGrantClient>,
    ) -> AppState {
        let user_requester = Arc::new(CachedRequester::new(
            request.clone(),
            "https://osu.ppy.sh/api/v2/users",
            24600,
        ));
        let beatmap_requester = Arc::new(CachedRequester::new(
            request.clone(),
            "https://osu.ppy.sh/api/v2/beatmaps",
            86400,
        ));

        let db = Arc::new(
            DatabaseClient::new()
                .await
                .expect("failed to initialize db connection"),
        );

        let activity_tracker = ActivityTracker::new(
            db.clone(),
            50,
            user_requester.clone(),
            beatmap_requester.clone(),
            credentials_grant_client,
        )
        .await
        // TODO: better handle errors
        .expect("failed to initialize activity tracker");

        AppState {
            db,
            request,
            jwt: JwtUtil::new_jwt(),
            user_requester,
            beatmap_requester,
            activity_tracker,
        }
    }
}

pub fn routes(state: Arc<AppState>) -> ApiRouter<Arc<AppState>> {
    ApiRouter::new()
        .api_route(
            "/search/map",
            get_with(handlers::osu_api::osu_beatmap_search, |op| op.tag("Search")),
        )
        .api_route(
            "/search/user/:query",
            get_with(handlers::osu_api::osu_user_search, |op| op.tag("Search")),
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
            "/leaderboard",
            get_with(handlers::leaderboard::get_leaderboard, |op| {
                op.tag("Leaderboard")
            }),
        )
}
