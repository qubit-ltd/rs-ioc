// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;

use qubit_ioc::CleanupError;
use qubit_ioc::Managed;

fn main() {
    let _managed = Managed::asynchronous_with_graceful_ticket(
        Arc::new(1_u8),
        |_| Ok::<(), CleanupError>(()),
        |_| Ok::<(), CleanupError>(()),
        |_, ()| Box::pin(async { Ok(()) }),
    )
    .with_graceful_stop(|_| Ok(()));
}
