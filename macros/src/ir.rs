// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow multiple-public-types
//! Validated declarations shared by the IoC attribute expanders.

use proc_macro2::Span;
use syn::Attribute;
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
    /// Struct component declaration.
    Component,
    /// Struct service declaration.
    Service,
    /// Struct repository declaration.
    Repository,
    /// Factory function declaration.
    Bean,
    /// Inline registration group declaration.
    Configuration,
    /// Configuration subtree component declaration.
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
    /// Validated component, service, or repository struct.
    Component(ComponentIr),
    /// Validated factory function.
    Bean(Box<BeanIr>),
    /// Validated inline configuration module.
    Configuration(ConfigurationIr),
    /// Validated configuration-backed properties struct.
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
    /// Validated optional component ID literal.
    pub(crate) id: Option<LitStr>,
    /// Trait object interfaces projected from the concrete component.
    pub(crate) binds: Vec<TypeTraitObject>,
    /// Whether unnamed requests prefer the binding.
    pub(crate) primary: bool,
    /// Ordering value used for collection injection.
    pub(crate) order: i32,
    /// Optional activation profile.
    pub(crate) profile: Option<LitStr>,
}

/// Original declaration location for generated static definition metadata.
pub(crate) struct SourceIr {
    /// Name of the annotated declaration.
    pub(crate) item: Ident,
    /// Source span used for generated diagnostics.
    pub(crate) span: Span,
}

/// A validated component, service, or repository struct.
pub(crate) struct ComponentIr {
    /// Original item retained for output.
    pub(crate) item: ItemStruct,
    /// Normalized binding metadata.
    pub(crate) options: BindingOptions,
    /// Field requests in declaration order.
    pub(crate) fields: Vec<FieldIr>,
    /// Original source identity for generated registration metadata.
    pub(crate) source: SourceIr,
}

/// A named field and its dependency request.
pub(crate) struct FieldIr {
    /// Field initialized by the generated factory.
    pub(crate) ident: Ident,
    /// Request used to construct the field value.
    pub(crate) dependency: DependencyIr,
    /// Conditions under which generated dependency and initialization code
    /// exists.
    pub(crate) conditions: Vec<Attribute>,
}

/// A named bean parameter and its dependency request.
pub(crate) struct ParamIr {
    /// Parameter supplied when invoking the original factory function.
    pub(crate) ident: Ident,
    /// Request used to obtain the parameter value.
    pub(crate) dependency: DependencyIr,
}

/// Cardinality and source of a field or parameter value.
pub(crate) enum DependencyKind {
    /// Exactly one binding is required.
    Required,
    /// A missing binding becomes `None`.
    Optional,
    /// All matching bindings are collected in order.
    All,
    /// A value is read from the configuration snapshot at the given path.
    Value { path: LitStr },
}

/// Normalized dependency preserving the exact requested type and optional ID.
pub(crate) struct DependencyIr {
    /// Selection or configuration operation to generate.
    pub(crate) kind: DependencyKind,
    /// Concrete or trait-object type requested by the declaration.
    pub(crate) requested_type: Type,
    /// Optional exact ID for single-value selection.
    pub(crate) id: Option<LitStr>,
}

/// Whether a bean returns a value or an Arc, and whether it returns a Result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputShape {
    /// Function returns the component value directly.
    Bare,
    /// Function returns a shared component handle directly.
    Arc,
    /// Function returns a fallible component value.
    ResultBare,
    /// Function returns a fallible shared component handle.
    ResultArc,
}

/// Normalized factory output, including the concrete component type.
pub(crate) struct OutputIr {
    /// Whether the function returns a value, `Arc`, or fallible form.
    pub(crate) shape: OutputShape,
    /// Whether the factory also returns explicit stop and optional wait
    /// actions.
    pub(crate) managed: bool,
    /// Concrete component type registered with the runtime.
    pub(crate) component_type: Type,
}

/// A validated synchronous or asynchronous bean function.
pub(crate) struct BeanIr {
    /// Original callable item retained in the expansion.
    pub(crate) item: ItemFn,
    /// Normalized registration properties.
    pub(crate) options: BindingOptions,
    /// Generated registration marker name, if explicitly selected.
    pub(crate) marker: Option<Ident>,
    /// Factory parameters and their requests in declaration order.
    pub(crate) params: Vec<ParamIr>,
    /// Normalized output type and shape.
    pub(crate) output: OutputIr,
    /// Original source identity for diagnostics.
    pub(crate) source: SourceIr,
}

/// An inline configuration module and its default profile.
pub(crate) struct ConfigurationIr {
    /// Original inline module containing the declarations.
    pub(crate) item: ItemMod,
    /// Default profile applied to direct beans without an override.
    pub(crate) profile: Option<LitStr>,
    /// Source identity retained for diagnostics.
    pub(crate) source: SourceIr,
}

/// A configuration-backed struct and the subtree to deserialize.
pub(crate) struct ConfigurationPropertiesIr {
    /// Original properties struct retained for expansion.
    pub(crate) item: ItemStruct,
    /// Configuration subtree prefix to deserialize.
    pub(crate) prefix: LitStr,
    /// Binding options for the generated component.
    pub(crate) options: BindingOptions,
    /// Source identity retained for registration metadata.
    pub(crate) source: SourceIr,
}
