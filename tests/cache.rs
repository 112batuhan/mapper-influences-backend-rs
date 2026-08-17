use std::sync::Arc;
use std::time::Duration;

use mapper_influences_backend_rs::cache::RedisCache;
use serde::{Deserialize, Serialize};
use testcontainers_modules::{
    redis::{Redis, REDIS_PORT},
    testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt},
};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
struct Dummy {
    id: u32,
    name: String,
}

fn dummy(id: u32) -> Dummy {
    Dummy {
        id,
        name: format!("dummy {id}"),
    }
}

async fn init_cache() -> (Arc<RedisCache>, ContainerAsync<Redis>) {
    // The module defaults to redis 5.0, so the tag is pinned to the one in docker compose
    let container = Redis::default()
        .with_tag("7-alpine")
        .start()
        .await
        .expect("Failed to start redis test container");
    let host_port = container
        .get_host_port_ipv4(REDIS_PORT)
        .await
        .expect("Failed to get the redis container port");
    let cache = RedisCache::new(&format!("redis://127.0.0.1:{host_port}"))
        .await
        .expect("failed to initialize redis connection");
    (cache, container)
}

#[tokio::test]
async fn test_single_entry() {
    let (cache, _container) = init_cache().await;

    let missing: Option<Dummy> = cache.get("dummy:1").await.unwrap();
    assert_eq!(missing, None);

    cache.set("dummy:1", &dummy(1), 60).await.unwrap();
    let hit: Option<Dummy> = cache.get("dummy:1").await.unwrap();
    assert_eq!(hit, Some(dummy(1)));
}

#[tokio::test]
async fn test_multiple_entries() {
    let (cache, _container) = init_cache().await;

    // no keys at all shouldn't reach redis
    let empty = cache
        .get_multiple::<u32, Dummy>("dummy:", &[])
        .await
        .unwrap();
    assert!(empty.hits.is_empty());
    assert!(empty.misses.is_empty());

    cache
        .set_multiple("dummy:", &[(1, dummy(1)), (2, dummy(2))], 60)
        .await
        .unwrap();

    let result = cache
        .get_multiple::<u32, Dummy>("dummy:", &[1, 2, 3])
        .await
        .unwrap();
    assert_eq!(result.hits.len(), 2);
    assert_eq!(result.hits.get(&1), Some(&dummy(1)));
    assert_eq!(result.hits.get(&2), Some(&dummy(2)));
    assert_eq!(result.misses, vec![3]);
}

/// Entries written with a different shape are treated as misses instead of errors
#[tokio::test]
async fn test_unexpected_entry_shape() {
    let (cache, _container) = init_cache().await;

    cache.set("dummy:1", &"not a dummy", 60).await.unwrap();

    let hit: Option<Dummy> = cache.get("dummy:1").await.unwrap();
    assert_eq!(hit, None);
}

#[tokio::test]
async fn test_expiration() {
    let (cache, _container) = init_cache().await;

    cache.set("dummy:1", &dummy(1), 1).await.unwrap();
    cache
        .set_multiple("multiple:", &[(2, dummy(2))], 1)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let expired: Option<Dummy> = cache.get("dummy:1").await.unwrap();
    assert_eq!(expired, None);
    let expired_multiple = cache
        .get_multiple::<u32, Dummy>("multiple:", &[2])
        .await
        .unwrap();
    assert!(expired_multiple.hits.is_empty());
    assert_eq!(expired_multiple.misses, vec![2]);
}
