// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shares immutable component queries and then shuts down through one owner.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;

/// Builds the application and performs concurrent reads before shutdown.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let mut builder = ContainerBuilder::new();
    builder.register_managed_factory::<String, _>(&[], move |_| {
        let stops = Arc::clone(&captured);
        Ok(Managed::new(Arc::new(String::from("shared")), move |_| {
            stops.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }))
    })?;

    let context = Arc::new(builder.build_all()?);
    let expected = context.get::<String>()?;
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let context = Arc::clone(&context);
            let expected = Arc::clone(&expected);
            scope.spawn(move || {
                let actual = context.get::<String>().expect("shared service");
                assert!(Arc::ptr_eq(&actual, &expected));
            });
        }
    });

    drop(expected);
    let context = Arc::try_unwrap(context).map_err(|_| "query handles remain")?;
    let mut shutdown = context.begin_shutdown();
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    runtime.block_on(shutdown.wait())?;
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    Ok(())
}
