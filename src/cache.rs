use std::{collections::HashMap, fmt::Display, hash::Hash, sync::Arc};

use redis::{aio::ConnectionManager, AsyncCommands, Client};
use serde::{de::DeserializeOwned, Serialize};
use tracing::warn;

use crate::error::AppError;

#[derive(Debug)]
pub struct MultipleCacheResults<K: Hash + Eq, V> {
    pub hits: HashMap<K, V>,
    pub misses: Vec<K>,
}

/// Redis backed cache. Values are stored as json and every entry gets a TTL,
/// so redis handles the expiration for us.
///
/// [`ConnectionManager`] is cheap to clone and reconnects on its own, that's why
/// every method clones it instead of locking a single connection.
pub struct RedisCache {
    connection: ConnectionManager,
}

impl RedisCache {
    pub async fn new(url: &str) -> Result<Arc<RedisCache>, AppError> {
        let client = Client::open(url)?;
        let connection = ConnectionManager::new(client).await?;
        Ok(Arc::new(RedisCache { connection }))
    }

    pub async fn get<V: DeserializeOwned>(&self, key: &str) -> Result<Option<V>, AppError> {
        let mut connection = self.connection.clone();
        let serialized: Option<String> = connection.get(key).await?;
        Ok(serialized.and_then(|serialized| deserialize_entry(key, &serialized)))
    }

    pub async fn set<V: Serialize>(
        &self,
        key: &str,
        value: &V,
        expire_in: u64,
    ) -> Result<(), AppError> {
        let mut connection = self.connection.clone();
        let serialized = serde_json::to_string(value)?;
        connection
            .set_ex::<_, _, ()>(key, serialized, expire_in)
            .await?;
        Ok(())
    }

    /// Single `MGET` for all the keys. Returned misses are meant to be requested
    /// from the actual source and then written back with [`RedisCache::set_multiple`].
    pub async fn get_multiple<K, V>(
        &self,
        key_prefix: &str,
        keys: &[K],
    ) -> Result<MultipleCacheResults<K, V>, AppError>
    where
        K: Display + Hash + Eq + Clone,
        V: DeserializeOwned,
    {
        let mut hits: HashMap<K, V> = HashMap::new();
        let mut misses: Vec<K> = Vec::new();

        // redis errors out on `MGET` without any keys
        if keys.is_empty() {
            return Ok(MultipleCacheResults { hits, misses });
        }

        let full_keys: Vec<String> = keys
            .iter()
            .map(|key| format!("{}{}", key_prefix, key))
            .collect();

        let mut connection = self.connection.clone();
        let serialized_values: Vec<Option<String>> = connection.mget(&full_keys).await?;

        for ((key, full_key), serialized) in keys.iter().zip(&full_keys).zip(serialized_values) {
            match serialized.and_then(|serialized| deserialize_entry(full_key, &serialized)) {
                Some(value) => {
                    hits.insert(key.clone(), value);
                }
                None => misses.push(key.clone()),
            }
        }

        Ok(MultipleCacheResults { hits, misses })
    }

    /// Writes every entry with the same TTL in a single pipeline
    pub async fn set_multiple<K: Display, V: Serialize>(
        &self,
        key_prefix: &str,
        entries: &[(K, V)],
        expire_in: u64,
    ) -> Result<(), AppError> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut pipeline = redis::pipe();
        for (key, value) in entries {
            let serialized = serde_json::to_string(value)?;
            pipeline
                .set_ex(format!("{}{}", key_prefix, key), serialized, expire_in)
                .ignore();
        }

        let mut connection = self.connection.clone();
        pipeline.query_async::<()>(&mut connection).await?;
        Ok(())
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
