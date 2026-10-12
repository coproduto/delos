//! A small byte-budgeted LRU for results that are expensive to recompute
//! (e.g. neural-network masks). Ordinary node outputs are deliberately *not*
//! cached: they are cheap to regenerate at the requested resolution, and
//! keeping them would multiply memory use for large images.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type Entry = (Arc<dyn Any + Send + Sync>, usize, u64);

pub struct Cache {
    budget: usize,
    inner: Mutex<(HashMap<u64, Entry>, u64)>,
}

impl Cache {
    pub fn new(budget: usize) -> Self {
        Self { budget, inner: Mutex::new((HashMap::new(), 0)) }
    }

    pub fn get<T: Any + Send + Sync>(&self, key: u64) -> Option<Arc<T>> {
        let mut g = self.inner.lock().unwrap();
        g.1 += 1;
        let tick = g.1;
        let e = g.0.get_mut(&key)?;
        e.2 = tick;
        e.0.clone().downcast::<T>().ok()
    }

    pub fn insert<T: Any + Send + Sync>(&self, key: u64, value: Arc<T>, bytes: usize) {
        let mut g = self.inner.lock().unwrap();
        g.1 += 1;
        let tick = g.1;
        g.0.insert(key, (value, bytes, tick));
        let mut total: usize = g.0.values().map(|e| e.1).sum();
        while total > self.budget && g.0.len() > 1 {
            let (&oldest, _) = g.0.iter().min_by_key(|(_, e)| e.2).unwrap();
            if oldest == key {
                break;
            }
            total -= g.0.remove(&oldest).unwrap().1;
        }
    }

    pub fn bytes(&self) -> usize {
        self.inner.lock().unwrap().0.values().map(|e| e.1).sum()
    }
}
