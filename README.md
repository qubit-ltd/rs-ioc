# qubit-ioc

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc.svg?color=blue)](https://crates.io/crates/qubit-ioc)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

`qubit-ioc` assembles shared Rust components at application startup. It checks
the full dependency graph before invoking factories, then publishes a read-only
`ApplicationContext`. Attribute macros offer a declarative entry point; the
same container also supports explicit registration.

## Intended users

Rust application authors who want constructor-style injection, explicit trait
bindings, and predictable startup errors across crates.

## Installation

This repository currently builds as version `0.1.0`; it has not been published
to crates.io. For a source checkout, use a path dependency:

```toml
[dependencies]
qubit-ioc = { version = "0.1", path = "../rs-ioc" }
```

After publication, replace the path with `qubit-ioc = "0.1"`.

The default features are `macros`, `inventory`, and `config`. To use only the
manual runtime, set `default-features = false`:

```toml
qubit-ioc = { version = "0.1", path = "../rs-ioc", default-features = false }
```

| Feature | Default | Purpose |
| --- | --- | --- |
| `macros` | Yes | `#[Component]`, `#[Service]`, `#[Repository]`, `#[Configuration]`, `#[ConfigurationProperties]`, and `#[bean]`. |
| `inventory` | Yes | Discover definitions linked from other crates with `discover()`. |
| `config` | Yes | Register a `qubit-config` snapshot and use `#[value]` or `#[ConfigurationProperties]`. |
| `reflect` | No | Reserved for optional metadata integration; currently adds no construction behavior. |

## Declarative assembly

Declare a concrete component and explicitly bind it to a trait. A service can
then request `Arc<dyn Trait>` by type. The macro cannot infer the trait binding
from a separate `impl` block, so `bind = dyn Greeting` is required.

```rust
use std::sync::Arc;
use qubit_ioc::{ApplicationContext, Component, Service};

trait Greeting: Send + Sync {
    fn text(&self) -> &'static str;
}

#[Component(bind = dyn Greeting, id = "example.greeting.english", primary)]
struct English;

impl Greeting for English {
    fn text(&self) -> &'static str { "hello" }
}

#[Service]
struct Greeter {
    greeting: Arc<dyn Greeting>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ApplicationContext::builder().discover()?;
    builder.root::<Greeter>();
    builder.root::<usize>();
    let context = builder.build()?;
    assert_eq!(context.get::<Greeter>()?.greeting.text(), "hello");
    Ok(())
}
```

For exact selection, use `#[inject(id = "...")]` on an `Arc<T>` field or bean
parameter. `Option<Arc<T>>` and `Vec<Arc<T>>` express optional and all-candidate
requests. `#[bean]` supports synchronous and asynchronous free functions;
asynchronous definitions require `build_async()`.

## Manual assembly

Explicit instances and factories work with `default-features = false`:

```rust
use std::sync::Arc;
use qubit_ioc::{ContainerBuilder, Dependency};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(String::from("hello")))?;
    builder.register_factory::<usize, _>(&[Dependency::of::<String>()], |context| {
        let message = context.get::<String>().expect("declared dependency");
        Ok(Arc::new(message.len()))
    })?;
    builder.root::<usize>();
    let context = builder.build()?;
    assert_eq!(*context.get::<usize>()?, 5);
    Ok(())
}
```

Each binding is identified by its Rust type and optional ID. IDs use
dot-separated ASCII segments such as `example.greeting.english`; each segment
starts with a letter and continues with letters, digits, or underscores. An
unnamed request selects a sole candidate or a unique `primary` binding.

## Limitations

The container provides application-wide shared instances. It does not provide
prototype or request scopes, hot reload, lifecycle hooks, circular proxies, or
dynamic-library discovery. Struct macros support named-field and unit structs;
other shapes can use manual factories. Runtime reflection is not used to
construct components. `qubit-spi` remains responsible for provider selection
and fallback; its registry or a selected service can be registered as a normal
IoC component. See [the complete design](doc/complete-design.zh_CN.md) for the
full API and diagnostic rules.

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
