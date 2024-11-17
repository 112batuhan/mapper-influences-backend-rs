use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use axum::async_trait;
use bytes::Bytes;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use futures::future::try_join_all;
use itertools::Itertools;
use mapper_influences_backend_rs::{
    error::AppError,
    osu_api::{
        request::{OsuApiRequestClient, Requester},
        AuthRequest, OsuAuthToken,
    },
};
use serde_json::Value;

const OSU_CACHE_BASE_PATH: &str = "tests/data";

#[derive(Debug)]
pub enum ClientMod {
    Replay,
    Record,
}

pub struct OsuApiTestClient {
    pub working_client: Arc<OsuApiRequestClient>,
    pub request_cache: RwLock<HashMap<String, Bytes>>,
    pub path: String,
    pub client_mod: ClientMod,
}

fn read_osu_request_cache(file_path: &str) -> Option<HashMap<String, Bytes>> {
    let file = File::open(file_path).ok()?;
    let mut decoder = GzDecoder::new(BufReader::new(file));
    let mut decompressed_data = Vec::new();
    decoder.read_to_end(&mut decompressed_data).ok()?;

    let deserialized: HashMap<String, Vec<u8>> = serde_json::from_slice(&decompressed_data).ok()?;

    Some(
        deserialized
            .into_iter()
            .map(|(k, v)| (k, Bytes::from(v)))
            .collect(),
    )
}

fn save_osu_request_cache(file_path: &str, cache: &HashMap<String, Bytes>) -> std::io::Result<()> {
    let file = File::create(file_path)?;
    let mut encoder = GzEncoder::new(BufWriter::new(file), Compression::default());

    let serializable_cache: HashMap<String, Vec<u8>> =
        cache.iter().map(|(k, v)| (k.clone(), v.to_vec())).collect();

    let serialized = serde_json::to_vec(&serializable_cache)?;
    encoder.write_all(&serialized)?;
    encoder.finish()?;
    Ok(())
}
impl OsuApiTestClient {
    pub fn new(working_client: Arc<OsuApiRequestClient>, label: &str) -> Arc<Self> {
        let path = format!("{}/{}", OSU_CACHE_BASE_PATH, label);
        let cache = read_osu_request_cache(&path);
        let client_mod = if cache.is_none() {
            ClientMod::Record
        } else {
            ClientMod::Replay
        };

        let cache = cache.unwrap_or_default();
        let request_cache = RwLock::new(cache);

        Arc::new(OsuApiTestClient {
            working_client,
            path,
            client_mod,
            request_cache,
        })
    }

    fn read_cache_lock(&self) -> Result<RwLockReadGuard<HashMap<String, Bytes>>, AppError> {
        self.request_cache.read().map_err(|_| AppError::RwLock)
    }
    fn write_cache_lock(&self) -> Result<RwLockWriteGuard<HashMap<String, Bytes>>, AppError> {
        self.request_cache.write().map_err(|_| AppError::RwLock)
    }

    pub fn save_cache(&self) -> Result<(), AppError> {
        if let ClientMod::Record = self.client_mod {
            let cache = self
                .read_cache_lock()
                .map_err(|_| AppError::RwLock)?
                .clone();
            save_osu_request_cache(&self.path, &cache)?;
        }
        Ok(())
    }
}

#[async_trait]
impl Requester for OsuApiTestClient {
    async fn get_request(&self, url: &str, token: &str) -> Result<Bytes, AppError> {
        match &self.client_mod {
            ClientMod::Replay => {
                let read_cache_lock = self.read_cache_lock()?;
                let bytes = read_cache_lock.get(url).unwrap_or_else(|| {
                    panic!(
                        "Missing cache entry in {} \
                        Please delete the cache file to record requests again",
                        self.path
                    )
                });
                Ok(bytes.clone())
            }

            ClientMod::Record => {
                let bytes = self.working_client.get_request(url, token).await?;
                self.write_cache_lock()?
                    .insert(url.to_string(), bytes.clone());
                Ok(bytes)
            }
        }
    }
    async fn post_request(&self, _url: &str, _body: AuthRequest) -> Result<Bytes, AppError> {
        unreachable!()
    }
    async fn get_client_credentials_token(&self) -> Result<OsuAuthToken, AppError> {
        match &self.client_mod {
            ClientMod::Replay => Ok(OsuAuthToken::test()),
            ClientMod::Record => Ok(self.working_client.get_client_credentials_token().await?),
        }
    }

    /// reimplementing the same function but with sorting to keep the cache keys in order
    /// we don't need to sort in production so it's better to add it here
    async fn request_multiple(
        self: Arc<Self>,
        base_url: &str,
        keys: &[u32],
        access_token: &str,
    ) -> Result<Vec<Value>, AppError> {
        let mut handlers = Vec::new();

        // this is where we add sorting
        for chunk_ids in &keys.iter().sorted().chunks(50) {
            let url = format!(
                "{}?{}",
                base_url,
                chunk_ids
                    .map(|id| format!("ids[]={}", id))
                    .collect::<Vec<_>>()
                    .join("&")
            );
            let access_token_string = access_token.to_string();
            let self_clone = Arc::clone(&self);

            let handler = tokio::spawn(async move {
                let response: Result<Vec<Value>, AppError> = self_clone
                    .deserialize_without_outer(url, access_token_string)
                    .await;
                response
            });
            handlers.push(handler);
        }

        try_join_all(handlers)
            .await?
            .into_iter()
            .try_fold(vec![], |mut acc, result| {
                acc.extend(result?);
                Ok(acc)
            })
    }
}
