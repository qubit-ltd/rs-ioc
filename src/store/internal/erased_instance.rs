// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Storage for constructed component values.

//! Type-erased shared component values.

use std::any::Any;
use std::sync::Arc;

/// An erased complete `Arc<T>`, including any trait-object vtable metadata.
pub(crate) type ErasedInstance = Arc<dyn Any + Send + Sync>;
