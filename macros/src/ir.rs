// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Validated declarations shared by the IoC attribute expanders.

use proc_macro2::Span;
use syn::Ident;
use syn::ItemFn;
use syn::ItemMod;
use syn::ItemStruct;
use syn::LitStr;
use syn::Type;
use syn::TypeTraitObject;

/// The attribute spelling that originated a declaration and its diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MacroKind {
    Component,
    Service,
    Repository,
    Bean,
    Configuration,
    ConfigurationProperties,
}

impl MacroKind {
    /// Returns the user's attribute spelling for diagnostic messages.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Component => "Component",
            Self::Service => "Service",
            Self::Repository => "Repository",
            Self::Bean => "bean",
            Self::Configuration => "Configuration",
            Self::ConfigurationProperties => "ConfigurationProperties",
        }
    }
}

/// A declaration after syntax and local semantic validation.
pub(crate) enum Declaration {
    Component(ComponentIr),
    Bean(Box<BeanIr>),
    Configuration(ConfigurationIr),
    ConfigurationProperties(ConfigurationPropertiesIr),
}

impl std::fmt::Debug for Declaration {
    /// Displays only the variant because `syn` syntax trees do not enable extra
    /// debug traits.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Component(_) => "Component",
            Self::Bean(_) => "Bean",
            Self::Configuration(_) => "Configuration",
            Self::ConfigurationProperties(_) => "ConfigurationProperties",
        };
        formatter.write_str(kind)
    }
}

/// Common binding properties; an omitted ID remains `None`.
pub(crate) struct BindingOptions {
    pub(crate) id: Option<LitStr>,
    pub(crate) binds: Vec<TypeTraitObject>,
    pub(crate) primary: bool,
    pub(crate) order: i32,
    pub(crate) profile: Option<LitStr>,
}

/// Original declaration location for generated static definition metadata.
pub(crate) struct SourceIr {
    pub(crate) item: Ident,
    pub(crate) span: Span,
}

/// A validated component, service, or repository struct.
pub(crate) struct ComponentIr {
    pub(crate) kind: MacroKind,
    pub(crate) item: ItemStruct,
    pub(crate) options: BindingOptions,
    pub(crate) fields: Vec<FieldIr>,
    pub(crate) source: SourceIr,
}

/// A named field and its dependency request.
pub(crate) struct FieldIr {
    pub(crate) ident: Ident,
    pub(crate) dependency: DependencyIr,
}

/// A named bean parameter and its dependency request.
pub(crate) struct ParamIr {
    pub(crate) ident: Ident,
    pub(crate) dependency: DependencyIr,
}

/// Cardinality and source of a field or parameter value.
pub(crate) enum DependencyKind {
    Required,
    Optional,
    All,
    Value { path: LitStr },
}

/// Normalized dependency preserving the exact requested type and optional ID.
pub(crate) struct DependencyIr {
    pub(crate) kind: DependencyKind,
    pub(crate) requested_type: Type,
    pub(crate) id: Option<LitStr>,
    pub(crate) span: Span,
}

/// Whether a bean returns a value or an Arc, and whether it returns a Result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputShape {
    Bare,
    Arc,
    ResultBare,
    ResultArc,
}

/// Normalized factory output, including the concrete component type.
pub(crate) struct OutputIr {
    pub(crate) shape: OutputShape,
    pub(crate) component_type: Type,
    pub(crate) error_type: Option<Type>,
    pub(crate) span: Span,
}

/// A validated synchronous or asynchronous bean function.
pub(crate) struct BeanIr {
    pub(crate) item: ItemFn,
    pub(crate) options: BindingOptions,
    pub(crate) marker: Option<Ident>,
    pub(crate) params: Vec<ParamIr>,
    pub(crate) output: OutputIr,
    pub(crate) source: SourceIr,
}

/// An inline configuration module and its default profile.
pub(crate) struct ConfigurationIr {
    pub(crate) item: ItemMod,
    pub(crate) profile: Option<LitStr>,
    pub(crate) source: SourceIr,
}

/// A configuration-backed struct and the subtree to deserialize.
pub(crate) struct ConfigurationPropertiesIr {
    pub(crate) item: ItemStruct,
    pub(crate) prefix: LitStr,
    pub(crate) options: BindingOptions,
    pub(crate) source: SourceIr,
}
