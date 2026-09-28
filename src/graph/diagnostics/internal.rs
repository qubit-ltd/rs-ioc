// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal storage and provenance types for graph diagnostics.

mod diagnostic_paths;
mod path_origin;

pub(crate) use diagnostic_paths::DiagnosticPaths;
pub(crate) use path_origin::PathOrigin;
