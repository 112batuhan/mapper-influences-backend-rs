use std::sync::Arc;

use aide::{axum::ApiRouter, openapi::OpenApi};
use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Extension, Json,
};
use mapper_influences_backend_rs::{routes, AppState};
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

    aide::gen::extract_schemas(true);
    let mut api = OpenApi::default();

    let cors = CorsLayer::new().allow_methods(Any).allow_origin(Any);

    let app = ApiRouter::new()
        .route(
            "/docs",
            get(|| async { Html(include_str!("elements-ui.html")).into_response() }),
        )
        .route(
            "/openapi.json",
            get(|Extension(api): Extension<Arc<OpenApi>>| async { Json(api).into_response() }),
        )
        .nest("/", routes(state.clone()))
        .finish_api(&mut api)
        //.layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(Extension(Arc::new(api)))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
