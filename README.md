# rs-ioc

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc.svg?color=blue)](https://crates.io/crates/qubit-ioc)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

`qubit-ioc` is planned as an application-level IoC container for assembling and sharing components during application startup.

## Intended users

Rust application authors who need to compose application components and their dependencies at startup.

## Installation

The `qubit-ioc` crate has not been published yet. After publication, add it to your application with Cargo:

```toml
[dependencies]
qubit-ioc = "0.1"
```

## Starting point

This repository currently contains only the project scaffold. It has no public container API yet, so there is no code example to use at this stage.

## Planned scope

The initial design is expected to cover explicit instance and factory registration, dependency resolution, missing and cyclic dependency errors with dependency paths, construction in dependency order, and shared instances. An SPI registry or a service created by it can be used as a component; backend selection and creation fallback remain the responsibility of `qubit-spi`. The structure leaves room for later procedural macros and optional `inventory` discovery.

## Limitations

These are design goals, not implemented or released capabilities. The public API and container behavior will be decided after the design phase.

## Testing

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

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./align-ci.sh` to format code and
`./ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)
