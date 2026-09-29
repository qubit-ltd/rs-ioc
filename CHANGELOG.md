# Changelog

## 0.2.0 — release candidate

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

`0.2.0` is a release candidate in this checkout; no registry release has been made.
