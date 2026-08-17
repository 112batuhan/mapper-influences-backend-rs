use std::io::Read;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use flate2::read::GzDecoder;
use mapper_influences_backend_rs::backup::{
    backup_key, http_url_from_ws, list_backups, prune, run_backup, upload, verify_dump,
    BackupStore, SurrealSource,
};
use reqwest::Client;
use rusty_s3::actions::{CreateBucket, GetObject};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use testcontainers_modules::{
    minio::MinIO,
    testcontainers::{
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
        ContainerAsync, GenericImage, ImageExt,
    },
};
use url::Url;

const SIGNATURE_LIFETIME: Duration = Duration::from_secs(300);
const MINIO_PORT: u16 = 9000;

/// Containers live as long as this does
struct TestBackupEnv {
    source: SurrealSource,
    store: BackupStore,
    client: Client,
    _surrealdb: ContainerAsync<GenericImage>,
    _minio: ContainerAsync<MinIO>,
}

async fn init_backup_env(retention: usize) -> TestBackupEnv {
    // Built by hand rather than with the surrealdb module, which waits for the
    // ready message on stderr. 2.x logs it to stdout, so the module times out.
    let surrealdb = GenericImage::new("surrealdb/surrealdb", "v2.6.5")
        .with_exposed_port(8000.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Started web server on "))
        .with_cmd(vec!["start", "--user", "backend", "--pass", "password"])
        .start()
        .await
        .expect("failed to start the surrealdb container");
    let surreal_port = surrealdb
        .get_host_port_ipv4(8000)
        .await
        .expect("failed to get the surrealdb port");

    let minio = MinIO::default()
        .start()
        .await
        .expect("failed to start the minio container");
    let minio_port = minio
        .get_host_port_ipv4(MINIO_PORT)
        .await
        .expect("failed to get the minio port");

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();

    let source = SurrealSource {
        http_url: format!("http://127.0.0.1:{surreal_port}"),
        username: "backend".to_string(),
        password: "password".to_string(),
        namespace: "prod".to_string(),
        database: "prod".to_string(),
    };

    let bucket = Bucket::new(
        Url::parse(&format!("http://127.0.0.1:{minio_port}")).unwrap(),
        UrlStyle::Path,
        "backups".to_string(),
        "auto".to_string(),
    )
    .unwrap();
    let credentials = Credentials::new("minioadmin", "minioadmin");

    let create = CreateBucket::new(&bucket, &credentials);
    let response = client.put(create.sign(SIGNATURE_LIFETIME)).send().await;
    assert!(
        response
            .expect("failed to reach minio")
            .status()
            .is_success(),
        "could not create the test bucket"
    );

    let store = BackupStore {
        bucket,
        credentials,
        prefix: "surrealdb".to_string(),
        retention,
    };

    TestBackupEnv {
        source,
        store,
        client,
        _surrealdb: surrealdb,
        _minio: minio,
    }
}

/// Enough records that the dump is comfortably past the minimum size check
async fn seed(env: &TestBackupEnv) {
    let mut query = String::from("DEFINE TABLE user SCHEMALESS;");
    for id in 1..=40 {
        query.push_str(&format!(
            "CREATE user:{id} SET username = 'mapper{id}', bio = 'a bio for mapper {id}';"
        ));
    }

    let response = env
        .client
        .post(format!("{}/sql", env.source.http_url))
        .basic_auth(&env.source.username, Some(&env.source.password))
        .header("surreal-ns", &env.source.namespace)
        .header("surreal-db", &env.source.database)
        .header("Accept", "application/json")
        .body(query)
        .send()
        .await
        .expect("failed to seed the database");
    assert!(response.status().is_success(), "seeding failed");
}

async fn download(env: &TestBackupEnv, key: &str) -> Vec<u8> {
    let action = GetObject::new(&env.store.bucket, Some(&env.store.credentials), key);
    let response = env
        .client
        .get(action.sign(SIGNATURE_LIFETIME))
        .send()
        .await
        .expect("failed to download the backup");
    assert!(response.status().is_success(), "backup object is missing");
    response.bytes().await.unwrap().to_vec()
}

#[tokio::test]
async fn test_backup_uploads_a_restorable_dump() {
    let env = init_backup_env(30).await;
    seed(&env).await;

    let outcome = run_backup(&env.client, &env.source, &env.store)
        .await
        .expect("the backup should succeed");

    assert!(outcome.key.starts_with("surrealdb/prod-prod-"));
    assert!(outcome.key.ends_with(".surql.gz"));
    assert!(
        outcome.uploaded_bytes < outcome.dump_bytes,
        "should compress"
    );

    // The dump has to come back out of the bucket intact and contain the data
    let downloaded = download(&env, &outcome.key).await;
    let mut dump = String::new();
    GzDecoder::new(&downloaded[..])
        .read_to_string(&mut dump)
        .expect("the uploaded backup should be valid gzip");

    verify_dump(&dump).expect("the round tripped dump should still verify");
    assert!(dump.contains("mapper1"), "the dump should hold the records");
    assert!(
        dump.contains("mapper40"),
        "the dump should hold every record"
    );
}

#[tokio::test]
async fn test_retention_keeps_only_the_newest() {
    let env = init_backup_env(2).await;

    // Five dumps, oldest first, named the way the real ones are
    for day in 1..=5 {
        let key = backup_key(
            &env.store.prefix,
            "prod",
            "prod",
            Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap(),
        );
        upload(&env.client, &env.store, &key, b"not a real dump".to_vec())
            .await
            .expect("upload should succeed");
    }
    assert_eq!(
        list_backups(&env.client, &env.store).await.unwrap().len(),
        5
    );

    let pruned = prune(&env.client, &env.store).await.expect("prune failed");
    assert_eq!(pruned, 3);

    let left = list_backups(&env.client, &env.store).await.unwrap();
    assert_eq!(left.len(), 2);
    assert!(left[0].contains("20260104"), "should keep the newest two");
    assert!(left[1].contains("20260105"), "should keep the newest two");
}

#[test]
fn test_verify_dump_rejects_broken_dumps() {
    let good = format!(
        "-- header\n\nOPTION IMPORT;\n\n{}\nINSERT [ {{}} ];",
        "-".repeat(600)
    );
    verify_dump(&good).expect("a complete dump should pass");

    verify_dump("OPTION IMPORT; INSERT [];").expect_err("a tiny dump should be rejected");

    let no_header = format!("{}\nINSERT [ {{}} ];", "-".repeat(600));
    verify_dump(&no_header).expect_err("a dump without the header should be rejected");

    let truncated = format!("OPTION IMPORT;\n{}\nINSERT [ {{ id: user", "-".repeat(600));
    verify_dump(&truncated).expect_err("a cut off dump should be rejected");
}

#[test]
fn test_http_url_from_ws() {
    assert_eq!(
        http_url_from_ws("ws://surrealdb-2x.railway.internal:8000/rpc"),
        "http://surrealdb-2x.railway.internal:8000"
    );
    assert_eq!(
        http_url_from_ws("wss://db.example.com/rpc"),
        "https://db.example.com"
    );
    assert_eq!(
        http_url_from_ws("ws://localhost:8100"),
        "http://localhost:8100"
    );
    // Already http, leave it alone
    assert_eq!(
        http_url_from_ws("http://localhost:8000"),
        "http://localhost:8000"
    );
}
