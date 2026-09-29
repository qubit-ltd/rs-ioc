// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Checks the registration protocol shared by every macro expansion.

use proc_macro2::TokenStream;
use quote::quote;

use crate::internal::RuntimePath;
use crate::ir::MacroKind;

/// Expands a validated declaration for an exact consumer dependency name.
fn expand(kind: MacroKind, options: TokenStream, item: TokenStream, root: &str) -> String {
    let runtime = RuntimePath::for_root(root);
    let parsed = crate::parse::parse(kind, options, item).expect("valid syntax");
    let declaration = crate::validate::validate(parsed, &runtime).expect("valid declaration");
    crate::expand::dispatch(declaration, &runtime)
        .expect("valid expansion")
        .to_string()
}

/// Verifies complete public registration without a second staging protocol.
fn assert_public(tokens: &str, ty: &str, method: &str) {
    assert!(
        tokens.contains(&format!("Definition :: < {ty} > :: builder ()")),
        "{tokens}"
    );
    for member in [
        ". source (",
        ". binding (",
        ". dependencies (",
        ". build () ?",
        "register_definition (",
    ] {
        assert!(tokens.contains(member), "missing {member}: {tokens}");
    }
    assert!(tokens.contains(&format!(". {method} (")), "{tokens}");
    assert!(!tokens.contains("DefinitionDraft"), "{tokens}");
    for source in [
        "CARGO_PKG_NAME",
        "module_path !",
        "file !",
        "line !",
        "column !",
        "stringify !",
    ] {
        assert!(tokens.contains(source), "missing source {source}: {tokens}");
    }
}

#[test]
fn test_component_public_definition_preserves_dependencies_and_alias_options() {
    let tokens = expand(
        MacroKind::Component,
        quote!(id = "service", bind = dyn Api, primary, order = 9, profile = "prod"),
        quote!(
            struct Service {
                input: ::std::sync::Arc<Input>,
            }
        ),
        "renamed_ioc",
    );
    assert_public(&tokens, "Service", "factory");
    for retained in [
        ":: r#renamed_ioc :: Definition",
        "Dependency :: of :: < Input >",
        "get :: < Input >",
        "bind :: < dyn Api , _ >",
        "primary : false",
        "order : 0",
        "primary : true",
        "order : 9",
        "service",
        "prod",
    ] {
        assert!(tokens.contains(retained), "missing {retained}: {tokens}");
    }
}

#[test]
fn test_bean_public_definition_all_factory_kinds_and_runtime_paths() {
    for root in ["qubit_ioc", "renamed_ioc", "type"] {
        let path: TokenStream = format!("::r#{root}").parse().expect("runtime path");
        for (item, method) in [
            (
                quote!(
                    fn create() -> ::std::sync::Arc<Service> {
                        todo!()
                    }
                ),
                "factory",
            ),
            (
                quote!(
                    async fn create() -> ::core::result::Result<::std::sync::Arc<Service>, std::io::Error> {
                        todo!()
                    }
                ),
                "async_factory",
            ),
            (
                quote!(fn create() -> #path::Managed<Service> { todo!() }),
                "managed_factory",
            ),
            (
                quote!(async fn create() -> ::std::result::Result<#path::Managed<Service>, std::io::Error> { todo!() }),
                "managed_async_factory",
            ),
        ] {
            let tokens = expand(
                MacroKind::Bean,
                quote!(type = Service, bind = dyn Api, primary, order = 7, id = "bean", profile = "prod"),
                item,
                root,
            );
            assert_public(&tokens, "Service", method);
            for retained in [
                "bind :: < dyn Api , _ >",
                "primary : false",
                "order : 0",
                "primary : true",
                "order : 7",
                "bean",
                "prod",
            ] {
                assert!(tokens.contains(retained), "missing {retained}: {tokens}");
            }
        }
    }
}

#[test]
fn test_cfg_and_config_dependency_projection_survives_public_registration() {
    let tokens = expand(
        MacroKind::Bean,
        quote!(),
        quote!(
            fn create(
                #[cfg(feature = "extra")] input: ::std::sync::Arc<Input>,
                #[value("app.label")] label: String,
                #[value("app.port")] port: u16,
            ) -> Service {
                todo!()
            }
        ),
        "type",
    );
    assert_public(&tokens, "Service", "factory");
    assert_eq!(tokens.matches("cfg (feature = \"extra\")").count(), 4);
    assert!(tokens.contains(". contains ("));
    assert!(tokens.contains("require_config !"));
    assert!(tokens.contains("codegen_v1 :: Config"));
    assert!(tokens.contains("Dependency :: of :: < Input >"));
    assert!(tokens.contains("get :: < Input >"));
}

#[test]
fn test_config_properties_public_definition_keeps_feature_diagnostic_bridge() {
    let tokens = expand(
        MacroKind::ConfigurationProperties,
        quote!(prefix = "app", id = "settings", primary, order = 5, profile = "prod"),
        quote!(
            struct Settings {
                port: u16,
            }
        ),
        "type",
    );
    assert_public(&tokens, "Settings", "factory");
    for retained in [
        ":: r#type :: __private :: require_config !",
        "codegen_v1 :: Config",
        "deserialize_properties_for",
        "settings",
        "primary : true",
        "order : 5",
        "prod",
    ] {
        assert!(tokens.contains(retained), "missing {retained}: {tokens}");
    }
}
