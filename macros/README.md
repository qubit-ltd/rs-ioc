# qubit-ioc-macros

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc-macros.svg?color=blue)](https://crates.io/crates/qubit-ioc-macros)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](../LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

This crate implements the procedural attributes used by `qubit-ioc`: `Component`,
`Service`, `Repository`, `Configuration`, `ConfigurationProperties`, and
`bean`. It generates explicit registration definitions; it does not provide the
runtime container.

Most applications should depend on `qubit-ioc` and enable its default `macros`
feature, which re-exports these attributes alongside the runtime API. The
expansions use the public `Definition` / `DefinitionBuilder` registration API.
Hidden configuration diagnostics and generated-code glue remain implementation
details. Use matching `0.3.0` runtime and macro versions.

## Installation

The workspace uses Rust 2024 and requires Rust 1.94 or newer. In an application
beside the repository checkout, depend on the runtime facade:

```toml
[dependencies]
qubit-ioc = { version = "0.3", path = "../rs-ioc", default-features = false, features = ["macros"] }
```

`macros` enables declarations and configuration groups. Reading configuration
with `#[value]` or `#[ConfigurationProperties]` additionally requires the
independent `config` feature and a registered configuration snapshot.

## Function beans and configuration groups

`#[bean] fn default_value() -> DefaultValue` generates `DefaultValueBean`;
install it with `builder.install::<DefaultValueBean>()?`. Override the marker
with `#[bean(marker = CustomFactory)]` and install `CustomFactory` instead.
A `#[Configuration]` module exports `register_ioc(&mut builder)` for its beans.
Registration stages definitions, while roots choose what to construct and
`context.get::<T>()?` reads the result. Grouping needs only `macros`.

The [complete function bean example](../examples/readme_beans.rs) demonstrates
all three forms. From the repository root, run:

```bash
cargo +1.94.0 run --example readme_beans --no-default-features --features macros --locked
```

Async beans require `build_async()` or `build_all_async()` and an application
executor. Return `Managed<T>` for explicit stop and wait actions; create the
resource inside its factory after graph validation. Configure a bounded
`WaitPolicy`, retain the returned `Application`, and call
`application.begin_shutdown(ShutdownMode::Graceful)` at normal exit, then await
the handle's `wait()`. Failure and cancellation use Immediate abort requests.
Both build variants return `BuildFailure` on failure without waiting for rollback:
inspect `cause()`, use `take_cleanup()` or `into_parts()`, and explicitly await
any cleanup handle's `wait()`. Owner, untransferred
`Managed`, and handle Drop request abort without waiting; query context Drop
does not close resources. See the [lifecycle and migration guide](../doc/lifecycle.md).

## Learn more

The API, supported declarations, configuration requirements, and runnable
examples are documented in the [project README](https://github.com/qubit-ltd/rs-ioc/blob/main/README.md), the
[English user guide](https://github.com/qubit-ltd/rs-ioc/blob/main/doc/user_guide.md), and the
[中文用户手册](https://github.com/qubit-ltd/rs-ioc/blob/main/doc/user_guide.zh_CN.md).
This repository contains the `0.3.0` release candidate; it has not been
published to crates.io.

## Testing

Run these workspace checks from the repository root. If your shell is in
`macros`, first change to its parent directory with `cd ..`:

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
./ci-check.sh

# Check code coverage
./coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](../LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./align-ci.sh` to format code and
`./ci-check.sh` to satisfy CI requirements before submitting a pull request.
Run both scripts from the repository root; see [align-ci.sh](../align-ci.sh)
and [ci-check.sh](../ci-check.sh).

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)
