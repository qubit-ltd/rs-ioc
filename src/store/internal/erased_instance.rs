// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Type-erased shared component values.

use std::any::Any;
use std::sync::Arc;

/// A component's complete shared handle after its concrete type is erased.
///
/// The `Arc<T>` is stored as the `Any` payload rather than erasing `T` alone,
/// so callers can recover the original shared handle with `Any` downcasting.
/// This preserves shared ownership and any trait-object vtable metadata while
/// requiring the component type to be `Send + Sync + 'static`.
pub(crate) type ErasedInstance = Arc<dyn Any + Send + Sync>;
