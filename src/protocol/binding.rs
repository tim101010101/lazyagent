use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Unique identity for a resolved agent session across poll cycles.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct SessionKey {
    pub pane_id: String,
    pub matched_pid: u32,
    pub process_start_time: u64,
}

pub struct SessionBindingStore {
    bindings: Mutex<HashMap<SessionKey, PathBuf>>,
}

impl SessionBindingStore {
    pub fn new() -> Self {
        Self {
            bindings: Mutex::new(HashMap::new()),
        }
    }

    /// Look up existing binding.
    pub fn get(&self, key: &SessionKey) -> Option<PathBuf> {
        self.bindings.lock().ok()?.get(key).cloned()
    }

    /// Store a new binding.
    pub fn bind(&self, key: SessionKey, jsonl_path: PathBuf) {
        if let Ok(mut map) = self.bindings.lock() {
            map.insert(key, jsonl_path);
        }
    }

    /// Remove binding (process gone or pane changed).
    pub fn unbind(&self, key: &SessionKey) {
        if let Ok(mut map) = self.bindings.lock() {
            map.remove(key);
        }
    }

    /// Evict bindings whose matched_pid no longer exists in the live process set.
    #[allow(dead_code)]
    pub fn evict_dead(&self, live_pids: &HashSet<u32>) {
        if let Ok(mut map) = self.bindings.lock() {
            map.retain(|k, _| live_pids.contains(&k.matched_pid));
        }
    }

    /// Check if a JSONL path is already bound by another session.
    pub fn is_bound_by_other(&self, path: &Path, exclude_key: &SessionKey) -> bool {
        if let Ok(map) = self.bindings.lock() {
            for (k, v) in map.iter() {
                if v == path && k != exclude_key {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(pane: &str, pid: u32, start: u64) -> SessionKey {
        SessionKey {
            pane_id: pane.into(),
            matched_pid: pid,
            process_start_time: start,
        }
    }

    #[test]
    fn test_bind_and_get() {
        let store = SessionBindingStore::new();
        let k = key("%0", 100, 1000);
        let path = PathBuf::from("/tmp/session.jsonl");

        assert!(store.get(&k).is_none());
        store.bind(k.clone(), path.clone());
        assert_eq!(store.get(&k), Some(path));
    }

    #[test]
    fn test_unbind() {
        let store = SessionBindingStore::new();
        let k = key("%0", 100, 1000);
        store.bind(k.clone(), PathBuf::from("/tmp/a.jsonl"));
        store.unbind(&k);
        assert!(store.get(&k).is_none());
    }

    #[test]
    fn test_evict_dead() {
        let store = SessionBindingStore::new();
        let alive = key("%0", 100, 1000);
        let dead = key("%1", 200, 2000);
        store.bind(alive.clone(), PathBuf::from("/tmp/a.jsonl"));
        store.bind(dead.clone(), PathBuf::from("/tmp/b.jsonl"));

        let mut live = HashSet::new();
        live.insert(100u32);
        store.evict_dead(&live);

        assert!(store.get(&alive).is_some());
        assert!(store.get(&dead).is_none());
    }

    #[test]
    fn test_is_bound_by_other() {
        let store = SessionBindingStore::new();
        let k1 = key("%0", 100, 1000);
        let k2 = key("%1", 200, 2000);
        let path = PathBuf::from("/tmp/shared.jsonl");

        store.bind(k1.clone(), path.clone());

        assert!(store.is_bound_by_other(&path, &k2));
        assert!(!store.is_bound_by_other(&path, &k1));
        assert!(!store.is_bound_by_other(Path::new("/tmp/other.jsonl"), &k2));
    }
}
