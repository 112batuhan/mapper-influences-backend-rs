//! Logical backups of the database, uploaded to an S3 compatible bucket.
//!
//! These are SurrealQL dumps rather than volume snapshots, and that's the point
//! of them: a dump is plain text that any surrealdb can import, so it survives
//! the storage format changing under the database. A snapshot of a volume whose
//! on-disk format moved on does not.
//!
//! This is both a module of the library and the `backup` binary: the functions
//! below are what the tests exercise, and [`main`] at the bottom is what the
//! schedule runs. It's a binary of its own rather than a routine inside the
//! server, so backups don't stop when the app is unhealthy and a deploy doesn't
//! interrupt one halfway.
//!
//! Being a binary crate root means nothing here may reach into the rest of the
//! library, which is why the errors below are its own rather than `AppError`.

use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use chrono::{DateTime, Utc};
use flate2::{write::GzEncoder, Compression};
use reqwest::Client;
use rusty_s3::actions::{DeleteObject, ListObjectsV2, PutObject};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use tracing::{error, info, warn};
use url::Url;

/// The backup never answers a request, so it has no business returning the
/// error type the api uses
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("could not compress the dump: {0}")]
    Compression(#[from] std::io::Error),

    #[error("the export endpoint answered with {0}")]
    Export(reqwest::StatusCode),

    #[error("the dump is not usable: {0}")]
    Verification(String),

    #[error("the bucket {0}")]
    Storage(String),
}

/// Presigned urls only have to outlive the request they were made for
const SIGNATURE_LIFETIME: Duration = Duration::from_secs(300);

/// A dump smaller than this isn't a dump of this database, it's a failure that
/// happened to come back with a 200
const MINIMUM_DUMP_BYTES: usize = 512;

/// Where the dump is read from. The export endpoint is plain http, even though
/// the app itself talks to the same server over websockets.
pub struct SurrealSource {
    pub http_url: String,
    pub username: String,
    pub password: String,
    pub namespace: String,
    pub database: String,
}

/// Where the dump is kept. `prefix` is the folder inside the bucket, `retention`
/// is how many of the most recent dumps are kept there.
pub struct BackupStore {
    pub bucket: Bucket,
    pub credentials: Credentials,
    pub prefix: String,
    pub retention: usize,
}

pub struct BackupOutcome {
    pub key: String,
    pub dump_bytes: usize,
    pub uploaded_bytes: usize,
    pub pruned: usize,
}

/// The app is configured with a websocket url, the export endpoint is http on
/// the same host, so the one config value covers both.
pub fn http_url_from_ws(url: &str) -> String {
    let url = url.trim_end_matches('/');
    let url = url.strip_suffix("/rpc").unwrap_or(url);

    if let Some(rest) = url.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = url.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        url.to_string()
    }
}

/// Timestamped so the keys sort oldest to newest, which is what the pruning relies on
pub fn backup_key(prefix: &str, namespace: &str, database: &str, now: DateTime<Utc>) -> String {
    let prefix = prefix.trim_end_matches('/');
    format!(
        "{}/{}-{}-{}.surql.gz",
        prefix,
        namespace,
        database,
        now.format("%Y%m%dT%H%M%SZ")
    )
}

pub async fn export_dump(client: &Client, source: &SurrealSource) -> Result<String, BackupError> {
    let url = format!("{}/export", source.http_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .basic_auth(&source.username, Some(&source.password))
        .header("surreal-ns", &source.namespace)
        .header("surreal-db", &source.database)
        .header("Accept", "application/octet-stream")
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        return Err(BackupError::Export(status));
    }

    Ok(response.text().await?)
}

/// A backup nobody checked is a guess. These are the cheap checks that catch the
/// realistic failures: an empty answer, an error page, a connection cut halfway.
pub fn verify_dump(dump: &str) -> Result<(), BackupError> {
    if dump.len() < MINIMUM_DUMP_BYTES {
        return Err(BackupError::Verification(format!(
            "it is {} bytes, expected at least {}",
            dump.len(),
            MINIMUM_DUMP_BYTES
        )));
    }
    if !dump.contains("OPTION IMPORT;") {
        return Err(BackupError::Verification(
            "it has no `OPTION IMPORT;` header, so it isn't a surrealql export".to_string(),
        ));
    }
    if !dump.trim_end().ends_with(';') {
        return Err(BackupError::Verification(
            "it doesn't end on a complete statement, it was cut short".to_string(),
        ));
    }
    Ok(())
}

pub fn compress(dump: &str) -> Result<Vec<u8>, BackupError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(dump.as_bytes())?;
    Ok(encoder.finish()?)
}

pub async fn upload(
    client: &Client,
    store: &BackupStore,
    key: &str,
    body: Vec<u8>,
) -> Result<(), BackupError> {
    let action = PutObject::new(&store.bucket, Some(&store.credentials), key);
    let url = action.sign(SIGNATURE_LIFETIME);

    let response = client.put(url).body(body).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(BackupError::Storage(format!(
            "rejected the upload of {} with {}",
            key, status
        )));
    }
    Ok(())
}

/// Oldest first, which is the order the pruning walks them in
pub async fn list_backups(
    client: &Client,
    store: &BackupStore,
) -> Result<Vec<String>, BackupError> {
    let mut action = ListObjectsV2::new(&store.bucket, Some(&store.credentials));
    action.with_prefix(store.prefix.trim_end_matches('/').to_string());
    let url = action.sign(SIGNATURE_LIFETIME);

    let response = client.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(BackupError::Storage(format!(
            "refused to list {} with {}",
            store.prefix, status
        )));
    }

    let body = response.text().await?;
    let parsed = ListObjectsV2::parse_response(&body).map_err(|error| {
        BackupError::Storage(format!("returned a listing we can't read: {error}"))
    })?;

    let mut keys: Vec<String> = parsed
        .contents
        .into_iter()
        .map(|object| object.key)
        .collect();
    keys.sort();
    Ok(keys)
}

/// Deletes everything past the most recent [`BackupStore::retention`] dumps
pub async fn prune(client: &Client, store: &BackupStore) -> Result<usize, BackupError> {
    let keys = list_backups(client, store).await?;
    let Some(over_retention) = keys.len().checked_sub(store.retention) else {
        return Ok(0);
    };

    let mut pruned = 0;
    for key in keys.iter().take(over_retention) {
        let action = DeleteObject::new(&store.bucket, Some(&store.credentials), key);
        let url = action.sign(SIGNATURE_LIFETIME);

        match client.delete(url).send().await {
            Ok(response) if response.status().is_success() => {
                info!("removed the expired backup {}", key);
                pruned += 1;
            }
            // A dump we failed to remove is only wasted space, it's not worth
            // failing an otherwise good backup over
            Ok(response) => warn!("could not remove {}: {}", key, response.status()),
            Err(error) => warn!("could not remove {}: {}", key, error),
        }
    }
    Ok(pruned)
}

/// Export, check, compress, upload, then drop whatever fell out of retention.
/// The pruning happens last on purpose, so a failed upload never costs us an
/// older dump that is still good.
pub async fn run_backup(
    client: &Client,
    source: &SurrealSource,
    store: &BackupStore,
) -> Result<BackupOutcome, BackupError> {
    info!(
        "exporting {}/{} from {}",
        source.namespace, source.database, source.http_url
    );
    let dump = export_dump(client, source).await?;
    verify_dump(&dump)?;

    let compressed = compress(&dump)?;
    let key = backup_key(
        &store.prefix,
        &source.namespace,
        &source.database,
        Utc::now(),
    );

    info!(
        "uploading {} ({} bytes, {} compressed)",
        key,
        dump.len(),
        compressed.len()
    );
    let uploaded_bytes = compressed.len();
    upload(client, store, &key, compressed).await?;

    let pruned = prune(client, store).await?;

    Ok(BackupOutcome {
        key,
        dump_bytes: dump.len(),
        uploaded_bytes,
        pruned,
    })
}

const DEFAULT_PREFIX: &str = "surrealdb";
const DEFAULT_RETENTION: usize = 30;
/// The bucket is across the network and the dump can be tens of megabytes
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("Missing {} environment variable", name))
}

/// Takes one backup and exits, so it can be run on a schedule. On Railway that's
/// a service built from `Dockerfile.backup` with a cron schedule, anywhere else
/// it's a binary you can run from cron, from CI, or by hand.
#[tokio::main]
pub async fn main() -> ExitCode {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // The same url the server uses, the export endpoint is http on the same host.
    // SURREAL_HTTP_URL overrides it for a database that isn't in the usual place.
    let http_url = std::env::var("SURREAL_HTTP_URL")
        .unwrap_or_else(|_| http_url_from_ws(&required("SURREAL_URL")));

    let source = SurrealSource {
        http_url,
        username: required("SURREAL_USER"),
        password: required("SURREAL_PASS"),
        namespace: std::env::var("SURREAL_NAMESPACE").unwrap_or_else(|_| "prod".to_string()),
        database: std::env::var("SURREAL_DATABASE").unwrap_or_else(|_| "prod".to_string()),
    };

    // R2 gives you the endpoint, and wants a region set without having any
    let endpoint = Url::parse(&required("R2_ENDPOINT")).expect("R2_ENDPOINT is not a valid url");
    let bucket = Bucket::new(
        endpoint,
        UrlStyle::Path,
        required("R2_BUCKET"),
        std::env::var("R2_REGION").unwrap_or_else(|_| "auto".to_string()),
    )
    .expect("could not build the bucket from R2_ENDPOINT and R2_BUCKET");

    let store = BackupStore {
        bucket,
        credentials: Credentials::new(
            required("R2_ACCESS_KEY_ID"),
            required("R2_SECRET_ACCESS_KEY"),
        ),
        prefix: std::env::var("BACKUP_PREFIX").unwrap_or_else(|_| DEFAULT_PREFIX.to_string()),
        retention: std::env::var("BACKUP_RETENTION")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_RETENTION),
    };

    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("could not build the http client");

    // A failed backup has to leave a non-zero exit code behind, otherwise the
    // schedule reports success and nobody finds out until a restore is needed
    match run_backup(&client, &source, &store).await {
        Ok(outcome) => {
            info!(
                "backed up {} ({} bytes, {} uploaded, {} old backups removed)",
                outcome.key, outcome.dump_bytes, outcome.uploaded_bytes, outcome.pruned
            );
            ExitCode::SUCCESS
        }
        Err(failure) => {
            error!("the backup failed: {}", failure);
            ExitCode::FAILURE
        }
    }
}
