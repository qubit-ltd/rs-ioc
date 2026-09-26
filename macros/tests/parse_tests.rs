// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#[allow(dead_code)]
#[path = "../src/ir.rs"]
mod ir;
#[path = "../src/parse.rs"]
mod parse;
#[path = "../src/validate.rs"]
mod validate;

use ir::Declaration;
use ir::DependencyKind;
use ir::MacroKind;
use ir::OutputShape;
use proc_macro2::TokenStream;
use quote::quote;

/// Parses an attribute declaration through the same validation boundary as a
/// macro entry.
fn declaration(kind: MacroKind, attributes: TokenStream, item: TokenStream) -> syn::Result<Declaration> {
    validate::validate(parse::parse(kind, attributes, item)?)
}

#[test]
fn test_component_accepts_id_and_repeated_bind() {
    let parsed = declaration(
        MacroKind::Component,
        quote!(id = "example.repository.memory", bind = dyn Send, bind = dyn Sync, primary),
        quote!(
            struct MemoryRepository;
        ),
    )
    .expect("valid component declaration should parse");
    let Declaration::Component(component) = parsed else {
        panic!("component IR was expected");
    };
    assert_eq!(
        component.options.id.expect("ID should be retained").value(),
        "example.repository.memory"
    );
    assert_eq!(component.options.binds.len(), 2);
    assert!(component.options.primary);
}

#[test]
fn test_bean_accepts_marker_and_result_arc_output() {
    let parsed = declaration(
        MacroKind::Bean,
        quote!(marker = RepositoryFactory, type = Repository),
        quote!(
            fn create() -> Result<Arc<Repository>, std::io::Error> {
                todo!()
            }
        ),
    )
    .expect("valid bean declaration should parse");
    let Declaration::Bean(bean) = parsed else {
        panic!("bean IR was expected");
    };
    assert_eq!(
        bean.marker.expect("marker should be retained").to_string(),
        "RepositoryFactory"
    );
    assert_eq!(bean.output.shape, OutputShape::ResultArc);
}

#[test]
fn test_bean_recognizes_standard_qualified_result_output() {
    for item in [
        quote!(
            fn create() -> std::result::Result<Arc<Repository>, std::io::Error> {
                todo!()
            }
        ),
        quote!(
            fn create() -> core::result::Result<Arc<Repository>, std::io::Error> {
                todo!()
            }
        ),
        quote!(
            fn create() -> ::std::result::Result<Arc<Repository>, std::io::Error> {
                todo!()
            }
        ),
    ] {
        let parsed = declaration(MacroKind::Bean, quote!(), item)
            .expect("standard qualified Result should parse as a fallible factory");
        let Declaration::Bean(bean) = parsed else {
            panic!("bean IR was expected");
        };
        assert_eq!(bean.output.shape, OutputShape::ResultArc);
        assert!(bean.output.error_type.is_some());
    }
}

#[test]
fn test_bean_does_not_assume_custom_result_path_is_standard_result() {
    let parsed = declaration(
        MacroKind::Bean,
        quote!(),
        quote!(
            fn create() -> company::Result<Arc<Repository>, std::io::Error> {
                todo!()
            }
        ),
    )
    .expect("custom result path is an opaque bare output type");
    let Declaration::Bean(bean) = parsed else {
        panic!("bean IR was expected");
    };
    assert_eq!(bean.output.shape, OutputShape::Bare);
}

#[test]
fn test_bean_rejects_nested_future_output() {
    for item in [
        quote!(
            fn create() -> Pin<Box<dyn Future<Output = Repository> + Send>> {
                todo!()
            }
        ),
        quote!(
            fn create() -> Result<Pin<Box<dyn Future<Output = Repository> + Send>>, std::io::Error> {
                todo!()
            }
        ),
        quote!(
            fn create() -> std::pin::Pin<Box<dyn std::future::Future<Output = Repository> + Send>> {
                todo!()
            }
        ),
    ] {
        let error = declaration(MacroKind::Bean, quote!(), item).expect_err("nested future return is unsupported");
        assert!(error.to_string().contains("future"), "{error}");
    }
}

#[test]
fn test_component_rejects_invalid_id() {
    for invalid in ["", "a..b", ".a", "a.", "2abc", "a-b", "a/b", "a b", "中文"] {
        let id = syn::LitStr::new(invalid, proc_macro2::Span::call_site());
        let error = declaration(
            MacroKind::Component,
            quote!(id = #id),
            quote!(
                struct MemoryRepository;
            ),
        )
        .expect_err("invalid segmented ID must be rejected");
        assert!(error.to_string().contains("id"), "{invalid}: {error}");
    }
}

#[test]
fn test_component_accepts_signed_order_bounds() {
    for (attributes, expected) in [
        (quote!(order = -2147483648), i32::MIN),
        (quote!(order = 2147483647), i32::MAX),
    ] {
        let parsed = declaration(
            MacroKind::Component,
            attributes,
            quote!(
                struct Ordered;
            ),
        )
        .expect("i32 order boundary should parse");
        let Declaration::Component(component) = parsed else {
            panic!("component IR was expected");
        };
        assert_eq!(component.options.order, expected);
    }
}

#[test]
fn test_component_rejects_order_out_of_range() {
    let error = declaration(
        MacroKind::Component,
        quote!(order = 2147483648),
        quote!(
            struct Ordered;
        ),
    )
    .expect_err("out of range order must be rejected");
    assert!(error.to_string().contains("32-bit"), "{error}");
}

#[test]
fn test_component_rejects_duplicate_and_unknown_options() {
    for attributes in [quote!(id = "a", id = "b"), quote!(qualifier = "a")] {
        let error = declaration(
            MacroKind::Service,
            attributes,
            quote!(
                struct Service;
            ),
        )
        .expect_err("duplicate or unknown option must be rejected");
        assert!(
            error.to_string().contains("duplicate") || error.to_string().contains("unknown"),
            "{error}"
        );
    }
}

#[test]
fn test_component_rejects_inject_name() {
    let error = declaration(
        MacroKind::Component,
        quote!(),
        quote!(
            struct Service {
                #[inject(name = "legacy")]
                dependency: Arc<Repository>,
            }
        ),
    )
    .expect_err("inject must use id rather than name");
    assert!(error.to_string().contains("id"), "{error}");
}

#[test]
fn test_component_rejects_id_on_collection() {
    let error = declaration(
        MacroKind::Component,
        quote!(),
        quote!(
            struct Service {
                #[inject(id = "repository")]
                dependencies: Vec<Arc<Repository>>,
            }
        ),
    )
    .expect_err("a collection cannot select a single ID");
    assert!(error.to_string().contains("Vec<Arc"), "{error}");
}

#[test]
fn test_component_rejects_duplicate_and_conflicting_field_helpers() {
    for item in [
        quote!(
            struct Service {
                #[inject]
                #[inject]
                dependency: Arc<Repository>,
            }
        ),
        quote!(
            struct Service {
                #[inject]
                #[value("repository")]
                dependency: Arc<Repository>,
            }
        ),
    ] {
        let error = declaration(MacroKind::Component, quote!(), item)
            .expect_err("duplicate or conflicting helper attributes must be rejected");
        assert!(
            error.to_string().contains("inject") || error.to_string().contains("value"),
            "{error}"
        );
    }
}

#[test]
fn test_component_records_injection_cardinality_and_value_path() {
    let parsed = declaration(
        MacroKind::Repository,
        quote!(),
        quote!(
            struct Service {
                one: Arc<Repository>,
                maybe: Option<Arc<Repository>>,
                all: Vec<Arc<Repository>>,
                #[value("service.timeout")]
                timeout: u64,
            }
        ),
    )
    .expect("supported field shapes should parse");
    let Declaration::Component(component) = parsed else {
        panic!("component IR was expected");
    };
    assert!(matches!(component.fields[0].dependency.kind, DependencyKind::Required));
    assert!(matches!(component.fields[1].dependency.kind, DependencyKind::Optional));
    assert!(matches!(component.fields[2].dependency.kind, DependencyKind::All));
    assert!(matches!(
        component.fields[3].dependency.kind,
        DependencyKind::Value { .. }
    ));
}

#[test]
fn test_bare_inject_on_field_and_bean_parameter_keeps_inferred_dependency() {
    let component = declaration(
        MacroKind::Component,
        quote!(),
        quote!(
            struct Service {
                #[inject]
                repository: Arc<Repository>,
            }
        ),
    )
    .expect("bare field injection should infer the required type");
    let Declaration::Component(component) = component else {
        panic!("component IR was expected");
    };
    assert!(matches!(component.fields[0].dependency.kind, DependencyKind::Required));
    assert!(component.fields[0].dependency.id.is_none());

    let bean = declaration(
        MacroKind::Bean,
        quote!(),
        quote!(
            fn create(#[inject] repository: Arc<Repository>) -> Service {
                todo!()
            }
        ),
    )
    .expect("bare parameter injection should infer the required type");
    let Declaration::Bean(bean) = bean else {
        panic!("bean IR was expected");
    };
    assert!(matches!(bean.params[0].dependency.kind, DependencyKind::Required));
    assert!(bean.params[0].dependency.id.is_none());
}

#[test]
fn test_inject_id_on_field_and_parameter_remains_exact_selection() {
    let component = declaration(
        MacroKind::Component,
        quote!(),
        quote!(
            struct Service {
                #[inject(id = "example.repository")]
                repository: Arc<Repository>,
            }
        ),
    )
    .expect("ID-selected field should parse");
    let Declaration::Component(component) = component else {
        panic!("component IR was expected");
    };
    assert_eq!(
        component.fields[0]
            .dependency
            .id
            .as_ref()
            .expect("ID should be preserved")
            .value(),
        "example.repository"
    );

    let bean = declaration(
        MacroKind::Bean,
        quote!(),
        quote!(
            fn create(#[inject(id = "example.repository")] repository: Arc<Repository>) -> Service {
                todo!()
            }
        ),
    )
    .expect("ID-selected parameter should parse");
    let Declaration::Bean(bean) = bean else {
        panic!("bean IR was expected");
    };
    assert_eq!(
        bean.params[0]
            .dependency
            .id
            .as_ref()
            .expect("ID should be preserved")
            .value(),
        "example.repository"
    );
}

#[test]
fn test_bare_inject_rejects_unsupported_scalar_and_assignment_syntax() {
    for item in [
        quote!(
            struct Service {
                #[inject]
                count: u64,
            }
        ),
        quote!(
            struct Service {
                #[inject = "wrong"]
                repository: Arc<Repository>,
            }
        ),
    ] {
        declaration(MacroKind::Component, quote!(), item)
            .expect_err("unsupported injection target or attribute syntax must be rejected");
    }
}

#[test]
fn test_bean_rejects_unsupported_signature() {
    for item in [
        quote!(
            fn generic<T>() -> T {
                todo!()
            }
        ),
        quote!(
            unsafe fn unsafe_factory() -> Repository {
                todo!()
            }
        ),
        quote!(
            fn borrowed(value: &Repository) -> Repository {
                todo!()
            }
        ),
        quote!(
            fn opaque() -> impl Send {
                todo!()
            }
        ),
    ] {
        let error =
            declaration(MacroKind::Bean, quote!(), item).expect_err("unsupported bean signature must be rejected");
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn test_configuration_rejects_external_module() {
    let error = declaration(
        MacroKind::Configuration,
        quote!(),
        quote!(
            mod services;
        ),
    )
    .expect_err("Configuration requires an inline module");
    assert!(error.to_string().contains("inline"), "{error}");
}

#[test]
fn test_configuration_properties_accepts_empty_root_prefix() {
    let parsed = declaration(
        MacroKind::ConfigurationProperties,
        quote!(prefix = "", id = "service.config"),
        quote!(
            struct ServiceConfig {
                enabled: bool,
            }
        ),
    )
    .expect("empty prefix selects root configuration");
    let Declaration::ConfigurationProperties(properties) = parsed else {
        panic!("configuration properties IR was expected");
    };
    assert_eq!(properties.prefix.value(), "");
}
