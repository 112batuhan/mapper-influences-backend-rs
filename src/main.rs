use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Router,
};
use mapper_influences_backend_rs::{
    handlers::{self},
    AppState,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!(
                    "{}=debug,tower_http=debug,axum::rejection=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = Arc::new(AppState::new().await);

    let cors = CorsLayer::new().allow_methods(Any).allow_origin(Any);

    let app = Router::new()
        .route(
            "/osu-api/search/map",
            get(handlers::osu_api::osu_beatmap_search),
        )
        .route(
            "/osu-api/search/user/:query",
            get(handlers::osu_api::osu_user_search),
        )
        .route(
            "/osu-api/beatmap/:beatmap_id",
            get(handlers::osu_api::osu_beatmap),
        )
        .route("/osu-api/user/:user_id", get(handlers::osu_api::osu_user))
        .route("/influence", post(handlers::influence::add_influence))
        .route(
            "/influence/influences/:user_id",
            get(handlers::influence::get_user_influences),
        )
        .route(
            "/influence/mentions/:user_id",
            get(handlers::influence::get_user_mentions),
        )
        .route(
            "/influence/:influenced_to",
            delete(handlers::influence::delete_influence),
        )
        .route(
            "/influence/:influenced_to/map/:beatmap_id",
            patch(handlers::influence::add_influence_beatmap),
        )
        .route(
            "/influence/:influenced_to/map/:beatmap_id",
            delete(handlers::influence::remove_influence_beatmap),
        )
        .route(
            "/influence/:influenced_to/bio",
            patch(handlers::influence::update_influence_description),
        )
        .route("/users/me", get(handlers::user::get_me))
        .route("/users/:user_id", get(handlers::user::get_user))
        .route("/users/bio", patch(handlers::user::update_user_bio))
        .route(
            "/users/map/:beatmap_id",
            patch(handlers::user::add_user_beatmap),
        )
        .route(
            "/users/map/:beatmap_id",
            delete(handlers::user::delete_user_beatmap),
        )
        .route(
            "/user/influence-order",
            post(handlers::user::set_influence_order),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            handlers::auth::check_jwt_token,
        ))
        .route(
            "/oauth/osu-redirect",
            get(handlers::auth::osu_oauth2_redirect),
        )
        .route("/leaderboard", get(handlers::leaderboard::get_leaderboard))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
