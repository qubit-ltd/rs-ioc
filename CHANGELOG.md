# Changelog

## 0.3.0 — release candidate

Breaking lifecycle and definition changes from the 0.2 source line; this version
has not been published to crates.io. The [bilingual migration guide](doc/lifecycle.md)
([中文](doc/lifecycle.zh_CN.md)) gives every old-to-new call and exit path.

- Builds now return the unique `Application` owner; cloneable `ApplicationContext`
  handles are obtained through `application.context()`. Query clones do not block
  shutdown, and lookup does not imply that a service still admits work.
- `Application::begin_shutdown(ShutdownMode::Graceful)` requests draining when
  its handle is first polled; `Immediate` requests all aborts before returning.
  Selected managed graphs require explicit `WaitPolicy`; real applications use
  bounded grace and termination deadlines with an application-driven timer.
- `WaitPolicy::bounded_with_total` adds an optional application-wide shutdown
  budget. Its failure is available as `ShutdownReport::overall_failure()` and
  contributes to `is_success()`; blocking synchronous work remains uninterruptible.
- Synchronous manual factories can use `register_injected_factory` and
  `register_injected_managed_factory` with zero-to-eight typed arguments:
  `Arc<T>`, `Option<Arc<T>>`, and `Vec<Arc<T>>` derive required, optional, and
  collection dependencies. Existing manual and async registration remains available.
- Sync and async builds now immediately return `BuildFailure` with the original
  `cause()` and optional `take_cleanup()` / `into_parts()` handle. Applications
  explicitly wait to observe rollback. `BuildError::CleanupFailed` is removed.
- Dropping the owner, an untransferred `Managed`, or a shutdown handle requests
  best-effort abort without waiting. `ShutdownReport::incomplete()` records
  unconfirmed termination, not forced termination.
- `Definition::builder()` and `register_definition` expose complete atomic
  definitions, including factories and trait aliases. Macros use the same public
  core. Hidden `codegen_v1::DefinitionDraft` is removed; remaining
  `__private::codegen_v1` names serve only configuration diagnostics and
  generated-code glue.
- Per-type collection ordering is precomputed at context publication. This
  avoids repeated query sorting; no overall linear-time or production speed
  guarantee is implied.
- The EventBus 0.18 adapter uses non-blocking `request_shutdown` and awaits its
  generation-bound ticket with `wait_async()`. Synchronous `shutdown`, including
  Immediate, still waits and is unsuitable as a managed abort callback.


## 0.2.0 — historical release candidate

This release candidate prepares the first published release and permits breaking
changes from the earlier 0.1.0 source line.

- `ApplicationContext` now supports concurrent read-only queries through `Arc`. Shutdown still consumes the single context owner; applications release shared handles and recover it with `Arc::try_unwrap` first.
- `#[bean]` recognizes `Managed<T>` through the actual runtime dependency path, including renamed and raw-keyword dependency names. Arbitrary qualified paths remain opaque ordinary component outputs.
- Runtime and macro packages are versioned together at `0.2.0`.
- Internal Rust source ownership is reorganized by type. Public facade modules and root re-exports remain the supported import paths; downstream code should not depend on private source-file layout. The source migration map is:

  | Former owner | Current source owners |
  | --- | --- |
  | `src/key.rs` | `src/key/binding_id.rs`, `src/key/binding_key.rs` |
  | `src/dependency.rs` | `src/dependency/dependency.rs`, `src/dependency/dependency_cardinality.rs` |
  | `src/options.rs` | `src/options/binding_options.rs`, `src/options/definition_source.rs` |
  | `src/error.rs` | `src/error/invalid_binding_id.rs`, `factory_error.rs`, `registration_error.rs`, `build_error.rs`, `build_access_error.rs`, `resolve_error.rs`; `ConfigReadContext` moved to `src/error/internal/config_read_context.rs` |
  | `src/builder.rs` and `src/builder/config.rs` | `src/builder/component_definition.rs`, `src/builder/container_builder.rs`, `src/builder/container_builder/config.rs`, `src/builder/internal/replacement.rs`, `src/builder/internal/validation.rs` |
  | `src/binding.rs` | `src/binding/internal/pending_binding.rs`, `pending_binding_kind.rs`, `pending_definition.rs` |
  | `src/application_context.rs` | `src/application_context/internal/built_binding.rs`, `src/application_context/internal/query_index.rs` |
  | `src/graph.rs` and `src/graph/diagnostics.rs` | `src/graph/internal/{binding_location,resolved_dependency,validated_graph,node,edge}.rs`, `src/graph/diagnostics/internal/{diagnostic_paths,path_origin}.rs` |
  | `src/managed.rs` | `src/managed/managed/managed.rs`, `cleanup_error.rs`, `shutdown_{phase,failure,error,handle}.rs`, and `internal/{cleanup_action,cleanup_entry,cleanup_journal,wait}.rs` |
  | `src/store.rs` | `src/store/internal/{instance_store,erased_instance}.rs` |
  | `macros/src/entrypoint.rs` | `macros/src/entry.rs` |
  | `macros/src/runtime_path.rs` | `macros/src/internal/runtime_path.rs` |
  | `macros/src/ir.rs` | one type per file under `macros/src/internal/ir/` |
  | `macros/src/parse.rs` and `parse_tests.rs` | raw parser owners under `macros/src/parse/internal/`; tests under `macros/src/tests/parse_tests.rs` |
  | `macros/src/validate.rs` and `validate/validated_options.rs` | validation domains under `macros/src/validate/`; shared option state under `macros/src/validate/internal/validated_options.rs` |
  | `macros/src/expand/{context.rs,symbols.rs}` | `macros/src/expand/internal/{expansion_context,symbols}.rs` |

  These are repository source locations, not public Rust module paths. `ApplicationContext`, `ContainerBuilder`, `ComponentDefinition`, `Managed`, and shutdown types remain available from their documented facade or crate-root exports.
- `#[value]` and `ConfigurationProperties` keep direct, non-interpolating reads. Structured deserialization rejects unknown fields by default. Applications needing interpolation call `Config::get_interpolated` explicitly in a factory.

Historical note: `0.2.0` was an unpublished release candidate in its source checkout.
The current checkout targets unpublished `0.3.0`.
