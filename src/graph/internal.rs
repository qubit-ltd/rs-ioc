// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Graph model types separated from validation algorithms.

pub(in crate::graph) mod binding_index;
pub(in crate::graph) mod binding_location;
pub(in crate::graph) mod edge;
pub(in crate::graph) mod node;
pub(in crate::graph) mod resolved_dependency;
// Owns root selection, reachability closure, edge resolution, cycle detection,
// and the cycle-free binding order.
pub(in crate::graph) mod selection;
pub(in crate::graph) mod validated_graph;
