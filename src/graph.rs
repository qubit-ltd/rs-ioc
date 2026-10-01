// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Resolves active bindings and validates their dependency graph before
//! construction.

mod diagnostics;

pub(crate) use diagnostics::DiagnosticPaths;
pub(crate) use diagnostics::PathOrigin;

mod internal;

pub(crate) use internal::binding_location::BindingLocation;
pub(crate) use internal::resolved_dependency::ResolvedDependency;
pub(crate) use internal::validated_graph::ValidatedGraph;
