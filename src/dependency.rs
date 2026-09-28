// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Dependency requests declared by factories.

mod dependency_cardinality;
#[path = "dependency/dependency.rs"]
mod request;

pub use dependency_cardinality::DependencyCardinality;
pub use request::Dependency;
