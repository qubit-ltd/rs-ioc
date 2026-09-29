// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Historical consumer with caller-owned bounded EventBus shutdown.

use std::error::Error;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use ioc_application_consumer::build_application;
use qubit_event_bus::EventBus;
use qubit_event_bus::spi::ShutdownMode as EventBusShutdownMode;
use qubit_execution_services::ExecutionServices;
use qubit_fs_registry::FileSystemRegistry;
use qubit_ioc::BindingKey;
use qubit_ioc::ShutdownMode;
use tokio::runtime::Builder;

/// Runs the historical business task and explicitly waits for cleanup.
fn main() -> Result<(), Box<dyn Error>> {
    let runtime = Builder::new_multi_thread().enable_all().build()?;
    let application = runtime.block_on(build_application(runtime.handle().clone()))?;
    let context = application.context();
    let bus = context.get::<EventBus>()?;
    let same_bus = context.get::<EventBus>()?;
    assert!(Arc::ptr_eq(&bus, &same_bus));
    assert!(context.binding_sources(&BindingKey::of::<EventBus>(None)).is_some());
    let services = context.get::<ExecutionServices>()?;
    let file_systems = context.get::<FileSystemRegistry>()?;
    assert!(file_systems.is_empty());
    let result = runtime.block_on(services.spawn_io(async { Ok::<u8, io::Error>(43) })?)?;
    assert_eq!(result, 43);
    // This blocking legacy operation belongs to the caller, never Managed abort.
    bus.shutdown(EventBusShutdownMode::Graceful {
        timeout: Duration::from_secs(5),
    })?;
    services.shutdown();
    runtime.block_on(services.await_termination());
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let report = runtime.block_on(shutdown.wait())?;
    assert!(report.is_success(), "{report:?}");
    assert!(services.is_terminated());
    Ok(())
}
