// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private metadata retained by the application context.

mod built_binding;
mod context_inner;
mod query_index;

pub(crate) use built_binding::BuiltBinding;
pub(super) use context_inner::ContextInner;
