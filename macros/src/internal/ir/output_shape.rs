// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Output shape macro intermediate representation.

/// Whether a bean returns a value or an Arc, and whether it returns a Result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputShape {
    /// Function returns the component value directly.
    Bare,
    /// Function returns a shared component handle directly.
    Arc,
    /// Function returns a fallible component value.
    ResultBare,
    /// Function returns a fallible shared component handle.
    ResultArc,
}
