// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Explicit builder registration and validated construction.

mod component_definition;
mod container_builder;
mod internal;

pub use component_definition::ComponentDefinition;
pub use container_builder::ContainerBuilder;
pub(crate) use internal::validation::validate_dependencies;
pub(crate) use internal::validation::validate_options;
