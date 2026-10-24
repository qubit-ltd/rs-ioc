// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Flattened binding identity and registration metadata.

use crate::graph::internal::binding_location::BindingLocation;
use crate::key::BindingKey;
use crate::options::DefinitionSource;

/// One binding flattened from the active definitions for graph validation.
///
/// The node retains its definition location and diagnostic source alongside
/// its matching metadata, so selection and error reporting use the same
/// registration snapshot.
#[derive(Clone)]
pub(in crate::graph) struct Node {
    /// Definition and binding indices in the profile-filtered input.
    pub(in crate::graph) location: BindingLocation,
    /// Rust type namespace and optional identifier used for exact matching.
    pub(in crate::graph) key: BindingKey,
    /// Source metadata used to explain failures and break ordering ties.
    pub(in crate::graph) source: DefinitionSource,
    /// Whether an unnamed single-binding request may choose this candidate.
    pub(in crate::graph) primary: bool,
    /// Caller-specified ordering value for requests that select all candidates.
    pub(in crate::graph) order: i32,
}
