// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Resolves the runtime crate path shared by expansion domains.

use proc_macro2::TokenStream;
use syn::Result;

use crate::internal::RuntimePath;

/// Context shared by expansion domains without assuming the runtime crate
/// alias.
pub(crate) struct ExpansionContext {
    /// Absolute or crate-local path to the runtime crate in the consumer.
    #[allow(dead_code)] // The implementation tasks consume this resolved path.
    pub(crate) runtime: TokenStream,
}

impl ExpansionContext {
    /// Resolves the runtime path as named in the consuming crate's manifest.
    pub(crate) fn for_runtime(runtime_path: &RuntimePath) -> Result<Self> {
        let runtime = runtime_path.tokens();
        Ok(Self { runtime })
    }
}
