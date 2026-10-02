// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shares immutable component queries and then shuts down through one owner.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::WaitPolicy;
use tokio::runtime::Builder;
use tokio::time::sleep;

/// Builds the application and performs concurrent reads before shutdown.
fn main() -> Result<(), Box<dyn Error>> {
    let runtime = Builder::new_current_thread().enable_time().build()?;
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded(
        Duration::from_secs(30),
        Duration::from_secs(5),
        |duration| Box::pin(sleep(duration)),
    ));
    builder.register_managed_factory::<String, _>(&[], move |_| {
        let stops = Arc::clone(&captured);
        Ok(Managed::synchronous(Arc::new(String::from("shared")), move |_| {
            stops.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }))
    })?;

    let application = match builder.build_all() {
        Ok(application) => application,
        Err(failure) => {
            let (cause, cleanup) = failure.into_parts();
            eprintln!("Application construction failed: {cause}");
            if let Some(mut cleanup) = cleanup
                && let Err(error) = runtime.block_on(cleanup.wait())
            {
                eprintln!("Application cleanup failed: {error}; {:?}", error.report());
            }
            return Err(cause.into());
        }
    };
    let context = application.context().clone();
    let expected = match context.get::<String>() {
        Ok(expected) => expected,
        Err(error) => {
            let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
            if let Err(cleanup_error) = runtime.block_on(shutdown.wait()) {
                eprintln!(
                    "Application cleanup failed: {cleanup_error}; {:?}",
                    cleanup_error.report()
                );
            }
            return Err(error.into());
        }
    };
    thread::scope(|scope| {
        for _ in 0..4 {
            let context = context.clone();
            let expected = Arc::clone(&expected);
            scope.spawn(move || {
                let actual = context.get::<String>().expect("shared service");
                assert!(Arc::ptr_eq(&actual, &expected));
            });
        }
    });

    drop(expected);
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    runtime.block_on(shutdown.wait())?;
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    Ok(())
}
