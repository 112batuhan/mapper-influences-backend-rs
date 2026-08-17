use std::sync::Arc;

use aide::axum::routing::{delete_with, get_with, patch_with, post_with};
use aide::axum::ApiRouter;
use axum::middleware;
use axum::routing::any;
use cache::RedisCache;
use database::leaderboard::{LeaderboardBeatmap, LeaderboardUser};
use database::DatabaseClient;
use handlers::activity::ActivityTracker;
use handlers::graph_vizualizer::GraphCache;
use handlers::leaderboard::LeaderboardCache;
use jwt::JwtUtil;
use osu_api::cached_requester::CombinedRequester;
use osu_api::credentials_grant::CredentialsGrantClient;
use osu_api::request::Requester;

pub mod backup;
pub mod cache;
pub mod cache_update;
pub mod daily_update;
pub mod database;
pub mod documentation;
pub mod error;
pub mod handlers;
pub mod jwt;
pub mod osu_api;
pub mod retry;

/// Both are comfortably longer than the refresh interval of the cache they belong
/// to, so that a failed refresh cycle doesn't leave the endpoint with a cold cache
const LEADERBOARD_CACHE_EXPIRATION: u64 = 3 * cache_update::LEADERBOARD_REFRESH_INTERVAL.as_secs();
const GRAPH_CACHE_EXPIRATION: u64 = 3 * cache_update::GRAPH_REFRESH_INTERVAL.as_secs();

pub struct AppState {
    pub db: Arc<DatabaseClient>,
    pub request: Arc<dyn Requester>,
    pub jwt: JwtUtil,
    pub cache: Arc<RedisCache>,
    pub cached_combined_requester: Arc<CombinedRequester>,
    pub activity_tracker: Arc<ActivityTracker>,
    pub credentials_grant_client: Arc<CredentialsGrantClient>,
    pub user_leaderboard_cache: LeaderboardCache<LeaderboardUser>,
    pub beatmap_leaderboard_cache: LeaderboardCache<LeaderboardBeatmap>,
    pub graph_cache: GraphCache,
}

impl AppState {
    pub async fn new(
        request: Arc<dyn Requester>,
        credentials_grant_client: Arc<CredentialsGrantClient>,
        db: Arc<DatabaseClient>,
        cache: Arc<RedisCache>,
    ) -> Arc<AppState> {
        let cached_combined_requester =
            CombinedRequester::new(request.clone(), cache.clone(), "https://osu.ppy.sh");

        let activity_tracker = ActivityTracker::new(
            db.clone(),
            50,
            cached_combined_requester.clone(),
            credentials_grant_client.clone(),
        )
        .await
        // TODO: better handle errors
        .expect("failed to initialize activity tracker");

        Arc::new(AppState {
            db,
            request: request.clone(),
            jwt: JwtUtil::new_jwt(),
            cached_combined_requester,
            activity_tracker,
            credentials_grant_client,
            // These are kept warm by the routines in [`cache_update`], so the TTLs
            // only decide how long a stale entry is served when a refresh fails.
            // The graph one lands on the hour that was set for it upstream.
            user_leaderboard_cache: LeaderboardCache::new(
                cache.clone(),
                "leaderboard:user:",
                LEADERBOARD_CACHE_EXPIRATION,
            ),
            beatmap_leaderboard_cache: LeaderboardCache::new(
                cache.clone(),
                "leaderboard:beatmap:",
                LEADERBOARD_CACHE_EXPIRATION,
            ),
            graph_cache: GraphCache::new(cache.clone(), GRAPH_CACHE_EXPIRATION),
            cache,
        })
    }
}

pub fn routes(state: Arc<AppState>) -> ApiRouter<Arc<AppState>> {
    ApiRouter::new()
        .api_route(
            "/search/map",
            get_with(handlers::osu_search::osu_beatmap_search, |op| {
                op.tag("Search").description(
                    "osu! beatmap search. 
                    Use the same query parameters in official beatmap search",
                )
            }),
        )
        .api_route(
            "/search/map/:beatmap_id",
            get_with(handlers::osu_search::osu_singular_beatmap_serch, |op| {
                op.tag("Search").description(
                    "Returns a single map for manual beatmap id field. 
                    Don't confuse it with `/search/map` endpoint which doesn't 
                    have path parameter",
                )
            }),
        )
        .api_route(
            "/search/user/:query",
            get_with(handlers::osu_search::osu_user_search, |op| op.tag("Search")),
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
            "/influence/:influenced_to/map",
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
            "/users/map",
            patch_with(handlers::user::add_user_beatmap, |op| op.tag("User")),
        )
        .api_route(
            "/users/map/:beatmap_id",
            delete_with(handlers::user::delete_user_beatmap, |op| op.tag("User")),
        )
        .api_route(
            "/users/influence-order",
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
            get_with(handlers::auth::osu_oauth2_redirect, |op| {
                op.tag("Auth").response::<302, ()>()
            }),
        )
        .api_route(
            "/oauth/logout",
            get_with(handlers::auth::logout, |op| {
                op.tag("Auth").response::<200, ()>()
            }),
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
        .api_route(
            "/graph",
            get_with(handlers::graph_vizualizer::get_graph_data, |op| {
                op.tag("Graph")
            }),
        )
}
