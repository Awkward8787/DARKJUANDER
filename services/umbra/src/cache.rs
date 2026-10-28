//! Prefix cache partitioned by model, prefix, trust domain, and label set.
//!
//! A hit is an exact key match. Another trust domain cannot reuse the entry,
//! because the domain is part of the key. This slice records the hit. It does
//! not skip token generation; llama.cpp slot restore is planned.

#![forbid(unsafe_code)]

use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub model: String,
    pub prefix_hash: String,
    pub trust_domain: String,
    pub label_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CacheLookup {
    pub hit: bool,
    pub trust_domain: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: u64,
    /// Admin-only. Counts lookups whose prefix existed in some other domain.
    /// The lookup result itself stays a miss and does not name that domain.
    pub foreign_prefix_observations: u64,
}

#[derive(Default)]
pub struct Cache {
    entries: HashMap<CacheKey, u64>,
    hits: u64,
    misses: u64,
    foreign_prefix_observations: u64,
}

impl Cache {
    pub fn lookup(&mut self, key: CacheKey) -> CacheLookup {
        if let Some(hits) = self.entries.get_mut(&key) {
            *hits += 1;
            self.hits += 1;
            return CacheLookup {
                hit: true,
                trust_domain: key.trust_domain,
            };
        }
        let foreign = self.entries.keys().any(|existing| {
            existing.model == key.model
                && existing.prefix_hash == key.prefix_hash
                && existing.trust_domain != key.trust_domain
        });
        if foreign {
            self.foreign_prefix_observations += 1;
        }
        self.entries.insert(key.clone(), 0);
        self.misses += 1;
        CacheLookup {
            hit: false,
            trust_domain: key.trust_domain,
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            entries: self.entries.len() as u64,
            foreign_prefix_observations: self.foreign_prefix_observations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(domain: &str, labels: &str) -> CacheKey {
        CacheKey {
            model: "mid-local".into(),
            prefix_hash: "abc".into(),
            trust_domain: domain.into(),
            label_fingerprint: labels.into(),
        }
    }

    #[test]
    fn same_domain_hits_and_other_domain_misses() {
        let mut cache = Cache::default();
        assert!(!cache.lookup(key("a", "pub")).hit);
        assert!(cache.lookup(key("a", "pub")).hit);
        let other = cache.lookup(key("b", "pub"));
        assert!(!other.hit);
        assert_eq!(other.trust_domain, "b");
        assert!(!cache.lookup(key("a", "secret")).hit);
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 3);
        // The third miss is a different label set, and domain b already holds the prefix.
        assert_eq!(stats.foreign_prefix_observations, 2);
        assert_eq!(stats.entries, 3);
    }
}
