// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Explicit builder registration and validated construction.

// Owns the `ComponentDefinition` registration contract and its inherent
// implementation.
mod component_definition;
// Owns the `ContainerBuilder` state and its inherent implementation.
mod container_builder;
// Owns the private construction, validation, replacement, and path helpers.
mod internal;

pub use component_definition::ComponentDefinition;
pub use container_builder::ContainerBuilder;
pub(crate) use internal::validation::validate_dependencies;
pub(crate) use internal::validation::validate_options;
