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
    ///
    /// The manifest lookup itself already happened in [`RuntimePath::resolve`];
    /// this constructor only copies the resolved path, so it never re-reads the
    /// manifest and performs no IO beyond the token stream it builds.
    ///
    /// # Parameters
    ///
    /// `runtime_path` is borrowed for the duration of the call and is not
    /// retained by the returned context.
    ///
    /// # Returns
    ///
    /// A context whose `runtime` field holds the freshly built absolute path
    /// tokens for the runtime crate.
    ///
    /// # Errors
    ///
    /// This constructor is currently infallible and always yields `Ok`; the
    /// `Result` signature is kept so that a future manifest-derived lookup can
    /// report a [`syn::Error`] without changing every caller.
    pub(crate) fn for_runtime(runtime_path: &RuntimePath) -> Result<Self> {
        let runtime = runtime_path.tokens();
        Ok(Self { runtime })
    }
}
