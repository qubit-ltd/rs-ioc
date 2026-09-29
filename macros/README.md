# qubit-ioc-macros

This crate implements the procedural attributes used by `qubit-ioc`: `Component`,
`Service`, `Repository`, `Configuration`, `ConfigurationProperties`, and
`bean`. It generates explicit registration definitions; it does not provide the
runtime container.

Most applications should depend on `qubit-ioc` and enable its default `macros`
feature, which re-exports these attributes alongside the runtime API. The
runtime package also supplies the hidden code-generation contract used by the
expansions, so this crate is intended to be used with a matching `qubit-ioc`
version.

The API, supported declarations, configuration requirements, and runnable
examples are documented in the [project README](https://github.com/qubit-ltd/rs-ioc/blob/main/README.md), the
[English user guide](https://github.com/qubit-ltd/rs-ioc/blob/main/doc/user_guide.md), and the
[中文用户手册](https://github.com/qubit-ltd/rs-ioc/blob/main/doc/user_guide.zh_CN.md).
This repository contains the `0.2.0` release candidate; it has not been
published to crates.io.
