// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Validated declarations shared by the IoC attribute expanders.

#[path = "internal/ir.rs"]
mod internal;

pub(crate) use internal::BeanIr;
pub(crate) use internal::BindingOptions;
pub(crate) use internal::ComponentIr;
pub(crate) use internal::ConfigurationIr;
pub(crate) use internal::ConfigurationPropertiesIr;
pub(crate) use internal::Declaration;
pub(crate) use internal::DependencyIr;
pub(crate) use internal::DependencyKind;
pub(crate) use internal::FieldIr;
pub(crate) use internal::MacroKind;
pub(crate) use internal::OutputIr;
pub(crate) use internal::OutputShape;
pub(crate) use internal::ParamIr;
pub(crate) use internal::SourceIr;
