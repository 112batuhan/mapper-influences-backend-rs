use std::{collections::HashMap, fmt::Display, hash::Hash, sync::Arc, time::Duration};

use redis::{
    aio::{ConnectionManager, ConnectionManagerConfig},
    AsyncCommands, Client,
};
use serde::{de::DeserializeOwned, Serialize};
use tracing::{error, warn};

use crate::error::AppError;

/// The crate default is 500ms, which is tight enough that a leaderboard sized
/// payload or a slow moment on the redis side turns into a timeout.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(1);

/// The manager reconnects on its own, but it retries with a growing delay before
/// it reports failure, and a request waiting on that is a request nobody is served.
/// One retry is enough to ride out a blip, the rest is the manager's own business.
const CONNECTION_RETRIES: usize = 1;

/// Hard ceiling on any single cache operation, whatever the client is doing under
/// it. A cache that can't answer within this is worth skipping, the source of the
/// data is usually faster than waiting this out.
const OPERATION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct MultipleCacheResults<K: Hash + Eq, V> {
    pub hits: HashMap<K, V>,
    pub misses: Vec<K>,
}

/// Redis backed cache. Values are stored as json and every entry gets a TTL,
/// so redis handles the expiration for us.
///
/// Every operation here is best effort and none of them can fail a request. A
/// redis that is down, slow or holding entries we can't read anymore reads as a
/// cache miss, and the caller falls back to the source it was caching. That costs
/// the database the traffic the cache was absorbing, which is a much better
/// failure than serving errors.
///
/// [`ConnectionManager`] is cheap to clone and reconnects on its own, that's why
/// every method clones it instead of locking a single connection.
pub struct RedisCache {
    connection: ConnectionManager,
}

impl RedisCache {
    /// An unusable url is still fatal, that's a configuration mistake worth failing
    /// on. A redis we simply can't reach yet is not, the manager keeps reconnecting
    /// in the background and the cache starts working once it's back.
    pub async fn new(url: &str) -> Result<Arc<RedisCache>, AppError> {
        let client = Client::open(url)?;
        let config = ConnectionManagerConfig::new()
            .set_response_timeout(Some(RESPONSE_TIMEOUT))
            .set_connection_timeout(Some(CONNECTION_TIMEOUT))
            .set_number_of_retries(CONNECTION_RETRIES);

        let connection = match ConnectionManager::new_with_config(client.clone(), config.clone())
            .await
        {
            Ok(connection) => connection,
            Err(error) => {
                error!("could not reach redis on startup, serving without a warm cache until it answers: {error}");
                ConnectionManager::new_lazy_with_config(client, config)?
            }
        };

        Ok(Arc::new(RedisCache { connection }))
    }

    pub async fn get<V: DeserializeOwned>(&self, key: &str) -> Option<V> {
        let mut connection = self.connection.clone();
        let serialized: Option<String> = bounded(key, connection.get(key)).await?;
        serialized.and_then(|serialized| deserialize_entry(key, &serialized))
    }

    pub async fn set<V: Serialize>(&self, key: &str, value: &V, expire_in: u64) {
        let serialized = match serde_json::to_string(value) {
            Ok(serialized) => serialized,
            Err(error) => {
                error!("failed to serialize the cache entry {}: {}", key, error);
                return;
            }
        };

        let mut connection = self.connection.clone();
        bounded(
            key,
            connection.set_ex::<_, _, ()>(key, serialized, expire_in),
        )
        .await;
    }

    /// Single `MGET` for all the keys. Returned misses are meant to be requested
    /// from the actual source and then written back with [`RedisCache::set_multiple`].
    /// Anything we couldn't read, for whatever reason, comes back as a miss.
    pub async fn get_multiple<K, V>(
        &self,
        key_prefix: &str,
        keys: &[K],
    ) -> MultipleCacheResults<K, V>
    where
        K: Display + Hash + Eq + Clone,
        V: DeserializeOwned,
    {
        let mut hits: HashMap<K, V> = HashMap::new();
        let mut misses: Vec<K> = Vec::new();

        // redis errors out on `MGET` without any keys
        if keys.is_empty() {
            return MultipleCacheResults { hits, misses };
        }

        let full_keys: Vec<String> = keys
            .iter()
            .map(|key| format!("{}{}", key_prefix, key))
            .collect();

        let mut connection = self.connection.clone();
        let Some(serialized_values): Option<Vec<Option<String>>> =
            bounded(key_prefix, connection.mget(&full_keys)).await
        else {
            return MultipleCacheResults {
                hits,
                misses: keys.to_vec(),
            };
        };

        for ((key, full_key), serialized) in keys.iter().zip(&full_keys).zip(serialized_values) {
            match serialized.and_then(|serialized| deserialize_entry(full_key, &serialized)) {
                Some(value) => {
                    hits.insert(key.clone(), value);
                }
                None => misses.push(key.clone()),
            }
        }

        MultipleCacheResults { hits, misses }
    }

    /// Writes every entry with the same TTL in a single pipeline
    pub async fn set_multiple<K: Display, V: Serialize>(
        &self,
        key_prefix: &str,
        entries: &[(K, V)],
        expire_in: u64,
    ) {
        if entries.is_empty() {
            return;
        }

        let mut pipeline = redis::pipe();
        for (key, value) in entries {
            let full_key = format!("{}{}", key_prefix, key);
            match serde_json::to_string(value) {
                Ok(serialized) => {
                    pipeline.set_ex(full_key, serialized, expire_in).ignore();
                }
                Err(error) => error!(
                    "failed to serialize the cache entry {}: {}",
                    full_key, error
                ),
            }
        }

        let mut connection = self.connection.clone();
        bounded(key_prefix, pipeline.query_async::<()>(&mut connection)).await;
    }
}

/// Runs a cache operation under [`OPERATION_TIMEOUT`] and reports anything that
/// went wrong as a `None`, which every caller reads as a miss.
async fn bounded<T>(
    key: &str,
    operation: impl std::future::Future<Output = redis::RedisResult<T>>,
) -> Option<T> {
    match tokio::time::timeout(OPERATION_TIMEOUT, operation).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            warn!("cache operation on {} failed: {}", key, error);
            None
        }
        Err(_) => {
            warn!(
                "cache operation on {} gave up after {:?}",
                key, OPERATION_TIMEOUT
            );
            None
        }
    }
}

/// A cached entry that we can't deserialize anymore is treated as a miss. This
/// happens when the shape of a cached type changes while old entries are still alive.
fn deserialize_entry<V: DeserializeOwned>(key: &str, serialized: &str) -> Option<V> {
    match serde_json::from_str(serialized) {
        Ok(value) => Some(value),
        Err(error) => {
            warn!("failed to deserialize the cache entry {}: {}", key, error);
            None
        }
    }
}
