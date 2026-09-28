# Changelog

## 0.2.0 — release candidate

This release candidate prepares the first published release and permits breaking
changes from the earlier 0.1.0 source line.

- `ApplicationContext` now supports concurrent read-only queries through `Arc`. Shutdown still consumes the single context owner; applications release shared handles and recover it with `Arc::try_unwrap` first.
- `#[bean]` recognizes `Managed<T>` through the actual runtime dependency path, including renamed and raw-keyword dependency names. Arbitrary qualified paths remain opaque ordinary component outputs.
- Runtime and macro packages are versioned together at `0.2.0`.
- Internal Rust module paths are reorganized by type ownership. Code that imports implementation modules should move to the root re-exports for public types.
- `#[value]` and `ConfigurationProperties` keep direct, non-interpolating reads. Structured deserialization rejects unknown fields by default. Applications needing interpolation call `Config::get_interpolated` explicitly in a factory.

`0.2.0` is a release candidate in this checkout; no registry release has been made.
