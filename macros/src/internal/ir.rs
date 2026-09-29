// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Macro IR declarations split by normalized owner type.

#[path = "ir/bean.rs"]
pub(crate) mod bean;
#[path = "ir/binding_options.rs"]
pub(crate) mod binding_options;
#[path = "ir/component.rs"]
pub(crate) mod component;
#[path = "ir/configuration.rs"]
pub(crate) mod configuration;
#[path = "ir/configuration_properties.rs"]
pub(crate) mod configuration_properties;
#[path = "ir/declaration.rs"]
pub(crate) mod declaration;
#[path = "ir/dependency.rs"]
pub(crate) mod dependency;
#[path = "ir/dependency_kind.rs"]
pub(crate) mod dependency_kind;
#[path = "ir/field.rs"]
pub(crate) mod field;
#[path = "ir/macro_kind.rs"]
pub(crate) mod macro_kind;
#[path = "ir/output.rs"]
pub(crate) mod output;
#[path = "ir/output_shape.rs"]
pub(crate) mod output_shape;
#[path = "ir/param.rs"]
pub(crate) mod param;
#[path = "ir/source.rs"]
pub(crate) mod source;

pub(crate) use bean::BeanIr;
pub(crate) use binding_options::BindingOptions;
pub(crate) use component::ComponentIr;
pub(crate) use configuration::ConfigurationIr;
pub(crate) use configuration_properties::ConfigurationPropertiesIr;
pub(crate) use declaration::Declaration;
pub(crate) use dependency::DependencyIr;
pub(crate) use dependency_kind::DependencyKind;
pub(crate) use field::FieldIr;
pub(crate) use macro_kind::MacroKind;
pub(crate) use output::OutputIr;
pub(crate) use output_shape::OutputShape;
pub(crate) use param::ParamIr;
pub(crate) use source::SourceIr;
