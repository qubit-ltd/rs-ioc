// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Application assembly for the downstream execution services fixture.

use std::error::Error;
use std::sync::Arc;

use qubit_event_bus::EventBus;
use qubit_event_bus::EventBusConfig;
use qubit_event_bus::EventBusRegistry;
use qubit_execution_services::ExecutionServices;
use qubit_fs_registry::FileSystemRegistry;
use qubit_ioc::Application;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::WaitPolicy;
use qubit_ioc::bean;
use tokio::runtime::Handle;

/// Creates execution services and transfers shutdown ownership to the container.
#[bean(marker = ExecutionServicesBean)]
fn execution_services(runtime: Arc<Handle>) -> Result<Managed<ExecutionServices>, FactoryError> {
    let services = ExecutionServices::builder()
        .enable_io()
        .runtime((*runtime).clone())
        .build()
        .map_err(FactoryError::new)?;
    Ok(Managed::asynchronous(
        Arc::new(services),
        |services| {
            let _stop_report = services.stop();
            Ok(())
        },
        |services| {
            Box::pin(async move {
                services.await_termination().await;
                Ok(())
            })
        },
    ))
}

/// Creates the historical event bus from the application-local registry.
#[bean(marker = EventBusBean)]
fn event_bus(registry: Arc<EventBusRegistry>) -> Result<Arc<EventBus>, FactoryError> {
    // Historical EventBus has no nonblocking request/ticket protocol.
    let bus = registry
        .create(&EventBusConfig::default())
        .map_err(FactoryError::new)?;
    Ok(Arc::new(bus))
}

/// Builds the historical consumer; the caller owns EventBus shutdown.
///
/// The returned application owns ExecutionServices cleanup. Construction errors
/// retain their rollback handle for callers to inspect and explicitly wait.
///
/// # Parameters
///
/// - `runtime`: Tokio runtime handle used to construct execution services.
///
/// # Returns
///
/// The assembled application, with cleanup ownership attached to its managed
/// execution services.
///
/// # Errors
///
/// Returns an error if a component cannot be registered or asynchronous
/// application construction fails. Build failures retain their cleanup handle
/// in the returned error for explicit inspection and waiting.
pub async fn build_application(runtime: Handle) -> Result<Application, Box<dyn Error>> {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder.register_instance(Arc::new(runtime))?;
    builder.register_instance(Arc::new(FileSystemRegistry::default()))?;
    builder.install::<ExecutionServicesBean>()?;
    builder.register_factory::<EventBusRegistry, _>(&[], |_| {
        let registry = EventBusRegistry::with_local().map_err(FactoryError::new)?;
        registry.seal();
        Ok(Arc::new(registry))
    })?;
    builder.install::<EventBusBean>()?;
    builder.root::<ExecutionServices>();
    builder.root::<EventBus>();
    builder.root::<FileSystemRegistry>();
    Ok(builder.build_async().await?)
}
