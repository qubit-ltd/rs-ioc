// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Complete public component definitions and their consuming builders.

#[path = "definition/definition.rs"]
mod component_definition;
mod definition_builder;
mod internal;

pub use component_definition::Definition;
pub use definition_builder::DefinitionBuilder;
