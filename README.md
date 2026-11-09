# structured-log-field-merge

Merges contextual key-value fields from nested scopes into a single flat map for structured log records.

```rust
use structured_log_field_merge::FieldMap;

fn main() {
    // Service-wide defaults.
    let mut base = FieldMap::new();
    base.insert("service", "checkout");
    base.insert("env", "prod");

    // A request-scoped child inherits the defaults and overrides one of them.
    let mut request = base.scope();
    request.insert("service", "checkout-api");
    request.insert("request_id", "abc123");

    // Flatten to a plain BTreeMap for serialisation.
    let flat = request.into_flat();
    assert_eq!(flat.get("service"), Some(&"checkout-api".to_string()));
    assert_eq!(flat.get("env"), Some(&"prod".to_string()));
    assert_eq!(flat.get("request_id"), Some(&"abc123".to_string()));

    // Or render directly for a compact log line.
    println!("{}", base); // env=prod service=checkout
}
```

## Why this exists

Structured logging libraries usually want a flat map of fields, but application code naturally produces fields in layers: service defaults, request context, per-operation context. Threading a single mutable map through every layer leaks state between calls and makes it hard to reason about which value wins. This crate provides a tiny `FieldMap` that can be cloned into a child scope, mutated locally, and flattened on demand. The trade-off is that scopes are clones rather than parent pointers: merging is O(n) in the number of inherited fields, but the API stays free of lifetimes and the flattened result is a plain `BTreeMap<String, String>` that can be handed to any serialiser.

## Merge semantics

A scope is a clone of its parent. Fields inserted into a child override the parent's value for the same key. When you flatten, the child's value wins. This is the usual "innermost scope wins" rule.

## Awkward edge

Values are stored as `String`, not as a generic `T`. If you need to log integers or booleans without converting them, you will need to format them yourself before inserting. This is deliberate: log serialisers almost always want strings at the boundary, and keeping the map string-typed avoids a generic parameter that would infect every call site.

## API

- `FieldMap::new() -> FieldMap`
- `FieldMap::insert(&mut self, key: &str, value: &str) -> Option<String>`
- `FieldMap::get(&self, key: &str) -> Option<&String>`
- `FieldMap::remove(&mut self, key: &str) -> Option<String>`
- `FieldMap::len(&self) -> usize`
- `FieldMap::is_empty(&self) -> bool`
- `FieldMap::iter(&self) -> impl Iterator<Item = (&String, &String)>`
- `FieldMap::scope(&self) -> FieldMap`
- `FieldMap::into_flat(self) -> BTreeMap<String, String>`
- `FieldMap::as_flat(&self) -> &BTreeMap<String, String>`
- `impl Display for FieldMap`
