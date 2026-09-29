// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Deferred action variants used to construct or project a binding.

use crate::binding::AliasProjector;
use crate::binding::AsyncFactory;
use crate::binding::ErasedInstance;
use crate::binding::ManagedAsyncFactory;
use crate::binding::ManagedSyncFactory;
use crate::binding::SyncFactory;
use crate::key::BindingKey;

/// Deferred construction or projection action for a binding.
pub(crate) enum PendingBindingKind {
    /// A shared instance supplied before graph construction.
    Instance(
        /// Shared value erased while retaining the contained `Arc` type.
        ErasedInstance,
    ),
    /// A one-shot synchronous factory.
    SyncFactory(
        /// Callback consumed to construct the binding synchronously.
        SyncFactory,
    ),
    /// A one-shot asynchronous factory.
    AsyncFactory(
        /// Callback consumed to construct the binding asynchronously.
        AsyncFactory,
    ),
    /// A one-shot synchronous factory returning a managed component.
    ManagedSyncFactory(
        /// Callback consumed to construct the binding with cleanup actions.
        ManagedSyncFactory,
    ),
    /// A one-shot asynchronous factory returning a managed component.
    ManagedAsyncFactory(
        /// Async callback consumed to construct the binding with cleanup
        /// actions.
        ManagedAsyncFactory,
    ),
    /// A projection from a previously constructed concrete binding.
    Alias {
        /// Key of the concrete binding to project.
        target: BindingKey,
        /// Type checked projection to the alias type.
        project: AliasProjector,
    },
}
