use std::{net::SocketAddr, sync::Arc};

use aide::{axum::ApiRouter, openapi::OpenApi};
use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Extension, Json,
};
use axum_swagger_ui::swagger_ui;
use mapper_influences_backend_rs::{
    osu_api::{credentials_grant::CredentialsGrantClient, request::OsuApiRequestClient},
    routes, AppState,
};
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::fmt::format::FmtSpan;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    // initializing client wrappers and state
    let request = Arc::new(OsuApiRequestClient::new(10));
    let client_credential_client = CredentialsGrantClient::new(request.clone())
        .await
        .expect("Failed to initialize credentials grant client");
    let state = Arc::new(AppState::new(request, client_credential_client).await);

    aide::gen::on_error(|error| {
        println!("{error}");
    });
    aide::gen::extract_schemas(true);
    let mut api = OpenApi::default();

    // TODO: restrict this after full deployment
    let cors = CorsLayer::very_permissive();
    let compression = CompressionLayer::new()
        .gzip(true)
        .deflate(true)
        .zstd(true)
        .br(true);

    let app = ApiRouter::new()
        .route(
            "/graph-vis/2d",
            get(|| async { Html(include_str!("graph-2d.html")).into_response() }),
        )
        .route(
            "/graph-vis/3d",
            get(|| async { Html(include_str!("graph-3d.html")).into_response() }),
        )
        .route(
            "/swagger",
            get(|| async { Html(swagger_ui("./openapi.json")) }),
        )
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
        .layer(cors)
        .layer(compression)
        .layer(TraceLayer::new_for_http())
        .layer(Extension(Arc::new(api)))
        .with_state(state)
        .into_make_service_with_connect_info::<SocketAddr>();

    let port = std::env::var("PORT").expect("PORT enviroment variable is not set");
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", &port))
        .await
        .unwrap();
    info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
