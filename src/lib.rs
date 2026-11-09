//! Merge contextual key-value fields from nested scopes into a single flat map
//! for structured log records.
//!
//! The core type is [`FieldMap`], a wrapper around `BTreeMap<String, String>` that
//! supports nested scopes and deterministic merging. Values are stored as strings
//! because structured logs almost always serialise values to strings at the
//! boundary; keeping the map string-typed avoids a generic parameter that would
//! infect every call site.
//!
//! # Merge semantics
//!
//! Fields are inserted into the *innermost* scope first. When the same key is
//! present in multiple scopes, the value from the scope closest to the leaf wins
//! (last writer wins). This matches the usual log-context behaviour where a
//! request-scoped field overrides a service-wide default.
//!
//! # Example
//!
//! ```
//! use structured_log_field_merge::FieldMap;
//!
//! let mut map = FieldMap::new();
//! map.insert("service", "checkout");
//! {
//!     let mut request = map.scope();
//!     request.insert("service", "checkout-api");
//!     request.insert("request_id", "abc123");
//!     let flat = request.into_flat();
//!     assert_eq!(flat.get("service"), Some(&"checkout-api".to_string()));
//!     assert_eq!(flat.get("request_id"), Some(&"abc123".to_string()));
//! }
//! ```

use std::collections::BTreeMap;
use std::fmt;

/// A flat map of structured log fields.
///
/// Internally a `BTreeMap` is used so that iteration order is deterministic
/// (sorted by key). This makes serialised log lines stable and testable without
/// relying on insertion order, which is easy to get wrong when scopes are
/// merged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldMap {
    fields: BTreeMap<String, String>,
}

impl FieldMap {
    /// Create an empty field map.
    pub fn new() -> Self {
        FieldMap {
            fields: BTreeMap::new(),
        }
    }

    /// Insert a key-value pair into the map.
    ///
    /// If the key already exists, the previous value is returned. This mirrors
    /// `BTreeMap::insert` and is useful when a caller wants to know whether it
    /// shadowed an inherited field.
    pub fn insert(&mut self, key: &str, value: &str) -> Option<String> {
        self.fields.insert(key.to_owned(), value.to_owned())
    }

    /// Look up a field by key.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.fields.get(key)
    }

    /// Remove a field by key, returning its previous value if present.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.fields.remove(key)
    }

    /// Return the number of fields in the map.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Return `true` if the map contains no fields.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Iterate over the fields in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.fields.iter()
    }

    /// Create a nested scope that inherits the current fields.
    ///
    /// The returned [`FieldMap`] is a clone, so mutations inside the scope do
    /// not affect the parent. This is the intended use: push a scope, add or
    /// override fields, then flatten it into a log record.
    ///
    /// # Why clone instead of a linked parent?
    ///
    /// A parent pointer would avoid the clone but would require lifetime
    /// parameters and make the common "flatten now" operation walk the chain.
    /// Log scopes are short-lived and small, so the clone is cheap and keeps
    /// the API simple and `'static`.
    pub fn scope(&self) -> FieldMap {
        self.clone()
    }

    /// Consume the map and return the underlying flat `BTreeMap`.
    pub fn into_flat(self) -> BTreeMap<String, String> {
        self.fields
    }

    /// Return a reference to the underlying flat map.
    pub fn as_flat(&self) -> &BTreeMap<String, String> {
        &self.fields
    }
}

impl fmt::Display for FieldMap {
    /// Render the fields as `key=value` pairs separated by spaces, in key order.
    ///
    /// This is a compact, human-readable representation intended for debug
    /// output and log lines that do not need a full serialiser. Values are
    /// printed verbatim; callers who need escaping should serialise
    /// `as_flat()` themselves.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (key, value) in &self.fields {
            if !first {
                write!(f, " ")?;
            }
            first = false;
            write!(f, "{}={}", key, value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_map_is_empty() {
        let map = FieldMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
        assert_eq!(map.get("missing"), None);
    }

    #[test]
    fn insert_and_get_round_trip() {
        let mut map = FieldMap::new();
        assert_eq!(map.insert("service", "checkout"), None);
        assert_eq!(map.get("service"), Some(&"checkout".to_string()));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn insert_returns_previous_value_on_override() {
        let mut map = FieldMap::new();
        map.insert("service", "checkout");
        let previous = map.insert("service", "checkout-api");
        assert_eq!(previous, Some("checkout".to_string()));
        assert_eq!(map.get("service"), Some(&"checkout-api".to_string()));
    }

    #[test]
    fn scope_inherits_parent_fields() {
        let mut parent = FieldMap::new();
        parent.insert("service", "checkout");
        parent.insert("env", "prod");

        let child = parent.scope();
        assert_eq!(child.get("service"), Some(&"checkout".to_string()));
        assert_eq!(child.get("env"), Some(&"prod".to_string()));
    }

    #[test]
    fn scope_mutations_do_not_affect_parent() {
        let mut parent = FieldMap::new();
        parent.insert("service", "checkout");

        let mut child = parent.scope();
        child.insert("service", "checkout-api");
        child.insert("request_id", "abc123");

        assert_eq!(parent.get("service"), Some(&"checkout".to_string()));
        assert_eq!(parent.get("request_id"), None);
        assert_eq!(child.get("service"), Some(&"checkout-api".to_string()));
        assert_eq!(child.get("request_id"), Some(&"abc123".to_string()));
    }

    #[test]
    fn nested_scopes_override_inner_to_outer() {
        let mut root = FieldMap::new();
        root.insert("level", "root");
        root.insert("service", "checkout");

        let mut middle = root.scope();
        middle.insert("level", "middle");
        middle.insert("request_id", "req-1");

        let mut leaf = middle.scope();
        leaf.insert("level", "leaf");
        leaf.insert("span_id", "span-1");

        let flat = leaf.into_flat();
        assert_eq!(flat.get("level"), Some(&"leaf".to_string()));
        assert_eq!(flat.get("service"), Some(&"checkout".to_string()));
        assert_eq!(flat.get("request_id"), Some(&"req-1".to_string()));
        assert_eq!(flat.get("span_id"), Some(&"span-1".to_string()));
    }

    #[test]
    fn remove_deletes_field() {
        let mut map = FieldMap::new();
        map.insert("secret", "value");
        assert_eq!(map.remove("secret"), Some("value".to_string()));
        assert_eq!(map.remove("secret"), None);
        assert!(map.is_empty());
    }

    #[test]
    fn iter_is_sorted_by_key() {
        let mut map = FieldMap::new();
        map.insert("zebra", "z");
        map.insert("apple", "a");
        map.insert("mango", "m");

        let keys: Vec<&String> = map.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn display_renders_sorted_key_value_pairs() {
        let mut map = FieldMap::new();
        map.insert("b", "2");
        map.insert("a", "1");
        assert_eq!(format!("{}", map), "a=1 b=2");
    }

    #[test]
    fn empty_display_is_empty_string() {
        let map = FieldMap::new();
        assert_eq!(format!("{}", map), "");
    }

    #[test]
    fn clone_is_independent() {
        let mut original = FieldMap::new();
        original.insert("k", "v");
        let mut copy = original.clone();
        copy.insert("k", "changed");
        assert_eq!(original.get("k"), Some(&"v".to_string()));
        assert_eq!(copy.get("k"), Some(&"changed".to_string()));
    }

    #[test]
    fn as_flat_matches_into_flat() {
        let mut map = FieldMap::new();
        map.insert("a", "1");
        map.insert("b", "2");
        let flat_ref = map.as_flat().clone();
        let flat_owned = map.into_flat();
        assert_eq!(flat_ref, flat_owned);
    }
}
