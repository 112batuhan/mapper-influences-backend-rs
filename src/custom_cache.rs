use std::{
    hash::Hash,
    time::{Duration, Instant},
};

use cached::Cached;
use linked_hash_map::LinkedHashMap;

pub struct CustomCache<K: Hash + Eq, V> {
    store: LinkedHashMap<K, (Instant, V)>,
    expire_in: Duration,
}
impl<K: Hash + Eq, V> CustomCache<K, V> {
    pub fn new(expire_in: u32) -> CustomCache<K, V> {
        CustomCache {
            store: LinkedHashMap::new(),
            expire_in: Duration::from_secs(expire_in.into()),
        }
    }
    fn discard_expired(&mut self) {
        while let Some(front_entry) = self.store.front() {
            if front_entry.1 .0.elapsed() > self.expire_in {
                self.store.pop_front();
            } else {
                break;
            }
        }
    }
}

impl<K: Hash + Eq, V> Cached<K, V> for CustomCache<K, V> {
    fn cache_get<Q>(&mut self, k: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        self.discard_expired();
        self.store.get(k).map(|value| &value.1)
    }
    fn cache_get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        self.discard_expired();
        self.store.get_mut(k).map(|value| &mut value.1)
    }
    fn cache_get_or_set_with<F: FnOnce() -> V>(&mut self, k: K, f: F) -> &mut V {
        self.discard_expired();
        let value = self.store.entry(k).or_insert_with(|| (Instant::now(), f()));
        &mut value.1
    }
    fn cache_set(&mut self, k: K, v: V) -> Option<V> {
        self.store
            .insert(k, (Instant::now(), v))
            .map(|value| value.1)
    }
    fn cache_remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        self.store.remove(k).map(|value| value.1)
    }
    fn cache_clear(&mut self) {
        self.store.clear();
    }
    fn cache_reset(&mut self) {
        self.store = LinkedHashMap::new();
    }
    fn cache_size(&self) -> usize {
        self.store.len()
    }
}
