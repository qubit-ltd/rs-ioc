# Historical Annotation and Auto-Wiring Design for qubit-ioc

> **Historical draft; not a guide to the current API.** The current contract is [the current design](complete-design.md), and current usage is documented in the [English user guide](user_guide.md). This file preserves early design context. Linker-based automatic registration, reflection integration, and a `reflect` feature described or implied by this draft are not implemented.
>
> The current runtime requires explicit assembly: call `install::<D>()` or a provider crate's `register_ioc(&mut builder)`. Macros do not submit linker-level global registration entries.
>
> The APIs and examples below are historical proposals and may not compile or run. They describe the intended application-level design at the time, not the current implementation.

The [Chinese original](annotation-design.zh_CN.md) is retained as the source historical draft.

## 1. Goals

Application developers should be able to declare dependencies on components and factories, then assemble an application context with a small amount of startup code.

```rust,ignore
// Historical illustration only; this is not a runnable current-API example.
use std::sync::Arc;
use qubit_ioc::{ApplicationContext, Component, Service};

trait UserRepository: Send + Sync {
    fn find_name(&self, id: u64) -> Option<String>;
}

#[Component(bind = dyn UserRepository, id = "example.repository.memory", primary)]
struct MemoryUserRepository;

impl UserRepository for MemoryUserRepository {
    fn find_name(&self, id: u64) -> Option<String> { Some(format!("user-{id}")) }
}

#[Service]
struct UserService {
    repository: Arc<dyn UserRepository>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ApplicationContext::builder().discover()?;
    builder.root::<UserService>();
    let context = builder.build()?;
    let service = context.get::<UserService>()?;
    assert_eq!(service.repository.find_name(7).as_deref(), Some("user-7"));
    Ok(())
}
```

In this proposal, `#[Component]` and `#[Service]` generate registration metadata and type-safe construction code. `discover()` imports metadata linked into the final binary; `build()` validates the dependency graph, constructs all singletons, and publishes a read-only `ApplicationContext`. The lower-level builder can also register instances and factories explicitly.

The interaction model drew inspiration from [summer-rs component factories](https://summer-rs.github.io/docs/getting-started/component/), [shaku component and interface declarations](https://github.com/AzureMarker/shaku), and [dill annotation, interface, and collection injection](https://docs.rs/dill/latest/dill/). `qubit-ioc` has its own dependency keys, errors, and build process; these libraries are design references, not runtime dependencies.

## 2. Initial macro scope and responsibilities

| Declaration | Applies to | Proposed behavior |
| --- | --- | --- |
| `#[Component]` | Named-field or unit structs | Infer dependencies from fields and construct one shared component; may declare interface bindings. |
| `#[Service]`, `#[Repository]` | Same | Semantic aliases for `#[Component]`, with distinct diagnostic categories only. |
| `#[bean]` | Free functions | Infer dependencies from parameters and register a synchronous or asynchronous factory. |
| `#[Configuration]` | Inline modules | Group `#[bean]` declarations and provide a shared profile; no implicit runtime configuration object. |
| `#[ConfigurationProperties(prefix = "...")]` | Named-field structs | Parse a configuration subtree with `qubit-config` and expose it as a component. |
| `#[inject(...)]` | Component fields and bean parameters | Select a binding ID; with no arguments, explicitly mark ordinary injection. |
| `#[value("...")]` | Component fields and bean parameters | Read one typed value from `qubit-config::Config`. |

The proposal placed all macros in a separate `qubit-ioc-macros` proc-macro crate, re-exported by `qubit-ioc` under its `macros` feature. `#[Service]` and `#[Repository]` add no persistence or transaction semantics. Every component is an application-level shared singleton; a “bean” is created by a factory and stored in the context, not recreated on every lookup.

Naming convention: macros on structs, enums, or traits use CamelCase; macros on fields or functions use snake_case. `#[Configuration]` is also CamelCase although it annotates a module. The initial proposal has no IoC macros for enums or traits.

### 2.1 Struct components and field injection

In a struct annotated with `#[Component]`, `#[Service]`, or `#[Repository]`, an unmarked `Arc<T>` field requests injection by type. Proposed forms:

- `Arc<T>`: required and unique; when there are multiple candidates, resolve using the primary-candidate rule in section 4.
- `Option<Arc<T>>`: `None` if no candidate exists; ambiguity remains an error.
- `Vec<Arc<T>>`: all bindings for the interface, or an empty vector if none exist.
- `#[inject(id = "...")] Arc<T>`: select an exact binding ID.
- `#[value("path")] T`: read a typed value through `qubit-config`; this field is not a component dependency.

`T` may be a concrete type or a trait object satisfying `Send + Sync + 'static`. The macro generates `Arc<T>` lookups and a struct literal; the component need not implement `Clone` or `Reflect`. An ordinary scalar field without one of the supported injection forms or `#[value]` is a compile error suggesting a `#[bean]` factory. Field visibility is preserved: generated code lives in the declaration module and does not make private fields public. Tuple structs, generic components, and components with lifetime parameters are outside the initial macro input scope, but can use explicit factory registration.

```rust,ignore
// Historical illustration only; not a promise of current syntax or behavior.
#[Service]
struct SearchService {
    #[inject(id = "example.search.fast")]
    search: Arc<dyn SearchBackend>,
    audits: Vec<Arc<dyn AuditSink>>,
    #[value("search.page_size")]
    page_size: usize,
}
```

### 2.2 Factory functions and configuration modules

Use `#[bean]` when construction requires custom logic or asynchronous I/O. The proposal supports free functions returning `T`, `Arc<T>`, `Result<T, E>`, or `Result<Arc<T>, E>`, including corresponding `async fn` forms. `E` must satisfy `Error + Send + Sync + 'static`. The macro removes only parameter attributes it consumes and preserves the original function for ordinary calls. A return-type alias whose ordinary output type cannot be inferred syntactically may specify `#[bean(type = T)]`; an alias hiding the `Result` structure must be rewritten as an explicit `Result<T, E>` or use low-level factory registration. Unsupported signatures produce an error located at the function or parameter.

```rust,ignore
// Historical illustration only; not a runnable current-API example.
#[Configuration]
mod data_access {
    use super::*;

    #[bean]
    async fn db_pool(settings: Arc<DbSettings>) -> Result<DbPool, DbError> {
        DbPool::connect(&settings.url).await
    }

    #[bean(bind = dyn UserRepository, id = "example.repository.postgres", primary)]
    fn user_repository(pool: Arc<DbPool>) -> PgUserRepository {
        PgUserRepository::new(pool)
    }
}
```

`#[Configuration]` groups declarations and locates their source; beans inside and outside the module use the same registration process. It does not emulate Java proxy objects or intercept direct calls between bean functions: a direct call remains an ordinary Rust function call. Inject the target component as a function parameter to obtain the shared container instance.

Common component and bean attributes are `bind = dyn Trait`, `id = "..."`, `primary`, `order = integer`, and `profile = "..."`. `primary` and `order` apply to generated interface bindings, or to the component's own binding when `bind` is absent. Repeated `bind` attributes expose multiple interfaces. Binding IDs are checked separately in each interface type namespace. A module-level `profile` is the default for inner beans that do not specify one.

Binding ID syntax follows the `rs-model-metadata` [`#[Entity(id = "...")]` validation rules](../../rs-model-metadata/model-id/src/model_id.rs): a nonempty dot-separated string whose segments match `[A-Za-z][A-Za-z0-9_]*`. For example, `example.repository.postgres` is valid; empty strings, `.example`, `example..cache`, `example.1cache`, hyphens, and non-ASCII characters are invalid. The macro requires a string literal and reports span-aware errors for declaration IDs and `#[inject(id = "...")]`; handwritten registration validates the same rule at registration time. IDs are case-sensitive and preserved without normalization. Omitting `id` means an un-IDed binding; no ID is inferred from a Rust type or function name. Source names are diagnostic metadata only. The injection ID must exactly match the target binding ID.

### 2.3 Configuration injection

`qubit-config` is the configuration parsing layer. The application explicitly loads or merges sources, then passes the final `Config` snapshot to the builder. IoC does not prescribe filenames, environment-variable prefixes, or source precedence. `#[ConfigurationProperties]` generates a factory depending on `Config`, using `ConfigSerdeExt::deserialize::<T>(prefix)` to construct a typed object. The struct therefore needs `serde::de::DeserializeOwned + Send + Sync + 'static`.

```rust,ignore
// Historical illustration only; not a runnable current-API example.
use serde::Deserialize;
use qubit_ioc::ConfigurationProperties;

#[derive(Deserialize)]
#[ConfigurationProperties(prefix = "db")]
struct DbSettings {
    url: String,
    max_connections: u32,
}

let config = load_application_config()?; // The application chooses sources and precedence.
let context = ApplicationContext::builder()
    .with_config(config)
    .discover()?
    .build_async()
    .await?;
```

`#[value("path")] T` corresponds to `Config::get::<T>(path)`. Missing values, conversion, interpolation, or unknown-field errors retain the original `qubit-config` source, with IoC adding component and field context. Without `with_config`, configuration-dependent bindings fail with a missing `Config` dependency before any factory runs. Configuration is read once during startup; existing components do not refresh automatically when the source changes.

## 3. Startup and discovery

The proposed primary entry point is `ApplicationContext::builder()`. `discover()` gathers static registration entries submitted by macros in linked crates and imports them into the same underlying `ContainerBuilder`. A no-macro application can be built entirely with `register_instance`, `register_factory`, and `register_async_factory`.

Each macro declaration generates two entry points:

1. An explicit registration function or `ComponentDefinition::register(builder)` implementation, allowing manual installation when inventory is disabled.
2. With `inventory` enabled, a static entry pointing to that registration function for collection by `discover()`.

The macro-generated submission is wrapped by the runtime's hidden `submit_component!` macro. It submits a static item when the runtime enables `inventory`, and expands to nothing when that feature is disabled. Generated code must not use `#[cfg(feature = "inventory")]` in the consumer crate to inspect a dependency's feature: that cfg checks the consumer's own feature. `#[Configuration]` does not submit an additional entry for its children, avoiding duplicates with each `#[bean]`.

The application must ensure crates containing submissions enter the final linked artifact; declaring a dependency in `Cargo.toml` alone may not guarantee this. Cross-crate components can be kept linked from an application assembly module with `use provider_crate as _;`. `discover()` does not scan disks, module paths, or dynamic libraries at runtime.

Before invoking registration functions, discovery entries are sorted by a stable source key composed of package name, module path, file, and line/column information. This key is for diagnostics and deterministic ordering, not component identity. After profile filtering, macro-generated and handwritten bindings enter the same conflict check, dependency resolution, and topological ordering. Submitting the same exact binding key twice is an error; linker order does not select a winner.

For test or application overrides, the proposal provides `exclude_component::<T>()` (declared before discovery, skipping the component and generated interface aliases) and `replace_binding` (replacing an exact key after discovery). Replacement must name its target key. If generated aliases are affected, handle each alias separately or exclude the source component. There is no implicit override based on registration order.

## 4. Interface bindings, IDs, and collections

Component identity is `(Rust TypeId, optional binding ID)`. `#[Component(bind = dyn Trait)]` or `#[bean(bind = dyn Trait)]` adds an interface alias when registering the concrete component; the alias and concrete component share the same underlying `Arc`. Generated coercions let the compiler check that the concrete type implements the trait. A macro can repeat `bind` to expose multiple interfaces. The declared `id` is used by the concrete binding and each generated interface alias; trait-object injection looks up an alias by interface type and ID.

```rust,ignore
// Historical illustration only.
#[Component(bind = dyn Cache, id = "example.cache.local", primary)]
struct LocalCache;

#[Component(bind = dyn Cache, id = "example.cache.remote")]
struct RemoteCache;
```

An unqualified `Arc<dyn Cache>` selects the only candidate when there is one; with multiple candidates it requires exactly one `primary`. `#[inject(id = "example.cache.remote")]` selects that exact ID regardless of primary status. No candidates is a missing-binding error; multiple candidates without one unique primary are ambiguous; multiple primary candidates for one interface are a configuration conflict reported before build. IDs are compared only within the same type namespace, not globally across types.

`Vec<Arc<dyn Cache>>` receives every binding for that interface, sorted by ascending explicit `order`, then original ID, then source key. An omitted order is `0`; an un-IDed binding sorts before bindings with IDs. Inventory's raw iteration order must not determine collection order. `Option<Arc<T>>` converts only complete absence to `None`; it does not hide ambiguity, dependency cycles, or factory errors. Optional and collection dependencies become graph edges after candidate resolution, so present candidates still participate in cycle detection.

The initial proposal does not support fuzzy runtime lookup by ID or implicit ID rewriting from configuration. Runtime backend selection and fallback after creation failure remain the responsibility of `qubit-spi`.

## 5. Contract between macro expansion and the container core

The proposed proc-macro implementation has four stages to keep dependency inference out of generated code:

1. **Parse:** parse struct fields, function parameters, return shapes, and attributes into one component declaration representation; retain spans and source locations for syntax nodes.
2. **Validate:** reject unsupported struct shapes, generics and lifetimes, invalid IDs, duplicate attributes, invalid `primary`/`bind` combinations, and indeterminate return shapes. This stage checks a single declaration only; it does not pretend to know what components exist in other crates.
3. **Infer dependencies:** translate `Arc<T>`, `Option<Arc<T>>`, `Vec<Arc<T>>`, binding IDs, and `#[value]` into core request descriptions, retaining field or parameter names for diagnostics.
4. **Generate:** emit strongly typed construction code that preserves the original declaration, explicit registration entry points, and optional inventory submissions. Generated code calls only public registration semantics and a narrow hidden protocol.

For a struct component, the conceptual expansion is:

```rust,ignore
// Historical conceptual expansion; not final tokens or runnable code.
impl ComponentDefinition for UserService {
    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        builder.register_factory::<Self>(
            [Dependency::of::<dyn UserRepository>()],
            |context| {
                let repository = context.get::<dyn UserRepository>()?;
                Ok(Arc::new(Self { repository }))
            },
        )
    }
}

qubit_ioc::__private::submit_component!(
    RegistrationEntry::new(<UserService as ComponentDefinition>::register, source_location!())
);
```

Real generated code in the proposal uses the narrowly exported `qubit_ioc::__private::codegen_v1` protocol as needed, supporting dependency renaming and future migration; it must not glob-re-export runtime internals. The macro crate depends on syntax parsing libraries, not the runtime crate, to avoid a Cargo dependency cycle. Macros preserve user types, functions, and doc attributes while removing only field or parameter attributes they consume.

Macros generate strongly typed construction code from syntax, leaving trait coercion, `Send + Sync`, return types, and field visibility to the Rust compiler. A macro cannot scan components in other crates, so missing bindings, multiple implementations, and cross-declaration cycles are reported during graph validation after discovery. Runtime graph errors should be structured `BuildError` values; ordinary missing dependencies or cycles must not panic. Explicit panics inside functions propagate with normal Rust panic semantics.

## 6. Build, lifecycle, and failure

The core first combines handwritten and macro registrations, filters by active profiles, then resolves single, optional, collection, and interface-alias requests. Before the first factory call it checks duplicates, missing bindings, ambiguity, multiple primary candidates, and cycles. After validation, it constructs all components serially in deterministic topological order. `build_async()` accepts synchronous and asynchronous factories; `build()` returns `AsyncRequired` when an async factory exists, after graph validation but before any factory executes. No Tokio runtime is bundled or created implicitly during build.

Each binding is constructed once in a successful build. `ApplicationContext::get` clones an `Arc` and does not rerun a factory. `Arc<T>`, `Arc<dyn Trait>`, and aliases for the same component share object identity. A failed build publishes no partial context, but external side effects already performed by factories, such as database connections or file writes, are not guaranteed to roll back. If asynchronous build is cancelled, the current future and local state are dropped according to Rust rules and later factories do not start; applications must manage independently spawned tasks themselves.

Profiles are static startup filters: pair declarations such as `#[Component(profile = "prod")]` and `#[bean(profile = "test")]` with `builder.active_profiles(["prod"])`. If unset, only the `default` profile is active; unannotated components are active in every profile. Filtering occurs before dependency graph validation, and a dependency missing after filtering remains a build error. Conditional missing registrations, runtime refresh, and request-scoped lifetimes require separate future design.

## 7. Boundaries with qubit-reflect and qubit-spi

`qubit-reflect` provides type, field, and method metadata, plus checked dynamic construction. Its `ReflectRegistry` stores metadata, not component instances. The proposed IoC macros analyze their own Rust declarations and generate strongly typed construction code; they do not infer dependencies from reflected field lists and do not require business components to derive `Reflect`. If a concrete need arises, an association between `TypeDescriptor` and bindings can be designed separately. This draft has no optional integration layer or corresponding Cargo feature, and does not implicitly initialize a global reflection registry. This preserves the thread-safety boundary of `qubit-reflect` and keeps its internal generation protocol independent.

The `qubit-spi` registry can be injected as a component. A `#[bean]` factory may call SPI `resolve()` and `create_configured()` to register a selected backend as a shared component. IoC handles construction order and cross-component error context; provider selection, priority, and fallback stay in SPI. Neither integration requires the IoC core to depend directly on the corresponding crate under default features.

## 8. Package layout and feature switches

The proposal recommends two publishable units in one repository:

```text
rs-ioc/
├── Cargo.toml             # qubit-ioc runtime and workspace
├── src/                   # container core, discovery entry point, public re-exports
├── macros/                # qubit-ioc-macros proc-macro crate
├── doc/                   # design and usage documentation
└── tests/                 # cross-crate and macro diagnostic scenarios
```

The proposed default features are `macros + inventory + config`, allowing ordinary applications to use annotations, static discovery, and `qubit-config` integration. `default-features = false` keeps a pure explicit-assembly core; reflection metadata integration is not part of the proposed feature set. `qubit-ioc-macros` emits code through the runtime's versioned generation protocol and does not call the private `codegen_v3` of `qubit-reflect`.

At the time this draft was written, rs-ioc was still a single-crate skeleton; adding a workspace, macro crate, features, and dependencies was proposed work. See [the current design](complete-design.md) for the implemented state.

## 9. Diagnostics and acceptance

At compile time, macros should report unsupported field types and function return shapes, invalid attribute combinations, duplicate field attributes, and interface conversions they cannot prove, located at the relevant declaration. Build errors should retain at least the requested type, binding ID, registration source, and dependency path. Configuration and factory errors should preserve their original source chain.

Before delivery, compilable cross-crate scenarios should establish the following behavior:

1. `#[Component]`, `#[Service]`, and `#[Repository]` are automatically discovered in linked crates, and handwritten `ComponentDefinition::register` is equivalent to discovery.
2. Field injection, synchronous and asynchronous `#[bean]`, `#[Configuration]` grouping, and configuration properties injection construct each component once; lookups return the same `Arc`.
3. Two interface implementations can be selected by ID; an unqualified lookup follows the unique-primary rule, and ambiguity reports all candidate sources.
4. `Option<Arc<T>>` and `Vec<Arc<T>>` handle absence, ambiguity, and deterministic ordering as specified.
5. Profile filtering precedes graph validation; a dependency missing after exclusion has a clear path.
6. Duplicate keys are errors when macros and explicit registration are mixed; explicit replacement and exclusion act on exact keys or component sources.
7. Syntax errors are span-aware compile errors; cross-component cycles, missing bindings, and factory failures are structured build errors.
8. Components without `Reflect` can still be assembled; a `qubit-spi` failure chain is preserved in the outer `#[bean]` context.
9. With default features disabled, pure low-level registration and build do not depend on macros, inventory, configuration, or reflection.

These are target-contract acceptance criteria from the historical proposal. The repository skeleton at the time had not implemented them, and they were not verified by running tests. The current implementation contract is documented in [complete-design.md](complete-design.md).
