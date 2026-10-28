//! Integrity and confidentiality labels for UMBRA.
//!
//! A label is never lowered by [`Labels::join`]. Untrusted joined with trusted
//! stays untrusted. Secret joined with public stays secret. Only a person,
//! through a broker prompt that this crate does not implement, may declassify.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Whether the bytes came from a source the user trusts.
///
/// Ordering is least-restrictive to most-restrictive so [`Ord::max`] is a join.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Integrity {
    Trusted,
    Untrusted,
}

impl Integrity {
    pub fn join(self, other: Self) -> Self {
        self.max(other)
    }
}

/// How widely the bytes may travel.
///
/// Ordering is least-restrictive to most-restrictive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidentiality {
    Public,
    Private,
    Secret,
}

impl Confidentiality {
    pub fn join(self, other: Self) -> Self {
        self.max(other)
    }

    pub fn at_least(self, floor: Self) -> bool {
        self >= floor
    }
}

/// Labels attached to one span of text.
///
/// `origin` is a provenance string such as `user`, `web:https://example`, or
/// `file:/path`. `owner` is the principal the span is charged to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Labels {
    pub integrity: Integrity,
    pub confidentiality: Confidentiality,
    pub origin: String,
    pub owner: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            integrity: Integrity::Trusted,
            confidentiality: Confidentiality::Public,
            origin: "user".to_string(),
            owner: "unknown".to_string(),
        }
    }
}

impl Labels {
    pub fn user(owner: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            ..Self::default()
        }
    }

    /// Join is commutative, associative, and idempotent, and it never lowers a label.
    pub fn join(&self, other: &Self) -> Self {
        Self {
            integrity: self.integrity.join(other.integrity),
            confidentiality: self.confidentiality.join(other.confidentiality),
            origin: join_tokens(&self.origin, &other.origin),
            owner: join_owner(&self.owner, &other.owner),
        }
    }

    pub fn join_all<'a>(items: impl IntoIterator<Item = &'a Self>) -> Self {
        let mut iter = items.into_iter();
        let Some(first) = iter.next() else {
            return Self::default();
        };
        iter.fold(first.clone(), |acc, next| acc.join(next))
    }

    /// Web and email origins cannot be marked trusted by the caller.
    ///
    /// The origin string itself is still caller-asserted until a broker fetch
    /// exists. A caller who lies about the origin is outside this check.
    pub fn ingest(mut self) -> Self {
        let origin = self.origin.to_ascii_lowercase();
        let forced = origin.starts_with("web:")
            || origin.starts_with("email:")
            || origin.starts_with("http:")
            || origin.starts_with("https:");
        if forced {
            self.integrity = Integrity::Untrusted;
        }
        self
    }

    /// The authenticated principal owns the span, whatever the caller wrote.
    pub fn pin_owner(mut self, owner: &str) -> Self {
        self.owner = owner.to_string();
        self
    }

    /// Stable identity used as part of a cache key.
    pub fn fingerprint(&self) -> String {
        format!(
            "i={}|c={}|o={}|w={}",
            integrity_name(self.integrity),
            confidentiality_name(self.confidentiality),
            self.origin,
            self.owner
        )
    }
}

pub fn integrity_name(value: Integrity) -> &'static str {
    match value {
        Integrity::Trusted => "trusted",
        Integrity::Untrusted => "untrusted",
    }
}

pub fn confidentiality_name(value: Confidentiality) -> &'static str {
    match value {
        Confidentiality::Public => "public",
        Confidentiality::Private => "private",
        Confidentiality::Secret => "secret",
    }
}

fn join_tokens(left: &str, right: &str) -> String {
    let mut set = BTreeSet::new();
    for part in left.split(" + ").chain(right.split(" + ")) {
        let part = part.trim();
        if !part.is_empty() {
            set.insert(part.to_string());
        }
    }
    set.into_iter().collect::<Vec<_>>().join(" + ")
}

fn join_owner(left: &str, right: &str) -> String {
    if left.is_empty() {
        return right.to_string();
    }
    if right.is_empty() || left == right {
        return left.to_string();
    }
    let mut owners = BTreeSet::new();
    for part in [left, right] {
        let bare = part.strip_prefix("multiple:").unwrap_or(part);
        for owner in bare.split('+') {
            if !owner.is_empty() {
                owners.insert(owner.to_string());
            }
        }
    }
    format!(
        "multiple:{}",
        owners.into_iter().collect::<Vec<_>>().join("+")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_never_downgrades_and_is_commutative() {
        let trusted = Labels::user("agent-a");
        let untrusted = Labels {
            integrity: Integrity::Untrusted,
            confidentiality: Confidentiality::Secret,
            origin: "web:https://example.test".to_string(),
            owner: "agent-a".to_string(),
        };
        let joined = trusted.join(&untrusted);
        assert_eq!(joined.integrity, Integrity::Untrusted);
        assert_eq!(joined.confidentiality, Confidentiality::Secret);
        assert_eq!(joined, untrusted.join(&trusted));
        assert_eq!(joined.join(&joined), joined);
    }

    #[test]
    fn web_origin_forces_untrusted() {
        let labeled = Labels {
            integrity: Integrity::Trusted,
            confidentiality: Confidentiality::Public,
            origin: "web:tests/injection/email-exfil.html".to_string(),
            owner: "spoofed".to_string(),
        }
        .ingest()
        .pin_owner("agent-a");
        assert_eq!(labeled.integrity, Integrity::Untrusted);
        assert_eq!(labeled.owner, "agent-a");
    }

    #[test]
    fn public_plus_private_is_private() {
        let public = Labels::user("a");
        let private = Labels {
            confidentiality: Confidentiality::Private,
            ..Labels::user("a")
        };
        assert_eq!(
            public.join(&private).confidentiality,
            Confidentiality::Private
        );
        assert!(Confidentiality::Secret.at_least(Confidentiality::Private));
        assert!(!Confidentiality::Public.at_least(Confidentiality::Private));
    }
}
