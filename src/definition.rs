// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Complete public component definitions and their consuming builders.

// Owns the `Definition` production type and its inherent implementation.
#[path = "definition/definition.rs"]
mod component_definition;
// Owns the `DefinitionBuilder` production type and its inherent implementation.
mod definition_builder;
// Owns the private alias-draft helper type shared by the definition builders.
mod internal;

pub use component_definition::Definition;
pub use definition_builder::DefinitionBuilder;
