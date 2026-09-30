// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_ioc::ApplicationState;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::WaitPolicy;
use tokio::runtime::Builder;

#[test]
fn test_query_clone_does_not_prevent_owner_shutdown() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(7), move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("managed registration succeeds");
    let application = builder.build_all().expect("managed application builds");
    let shared = application.context().clone();
    assert_eq!(shared.state(), ApplicationState::Running);
    let report = application.begin_shutdown(ShutdownMode::Immediate).abandon();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(report.is_success());
    assert_eq!(shared.state(), ApplicationState::Closed);
    assert_eq!(*shared.get::<u32>().expect("closed context retains values"), 7);
}

#[test]
fn test_missing_wait_policy_prevents_every_factory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<u8, _>(&[], move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(1))
        })
        .expect("ordinary registration succeeds");
    let observed = Arc::clone(&calls);
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(Managed::new(Arc::new(7), |_| Ok(())))
        })
        .expect("managed registration succeeds");
    assert!(matches!(builder.build_all(), Err(failure) if matches!(failure.cause(), BuildError::MissingWaitPolicy)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_unselected_managed_factory_does_not_require_wait_policy() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance(Arc::new(1_u8))
        .expect("instance registration succeeds");
    builder
        .register_managed_factory::<u32, _>(&[], |_| {
            panic!("unselected factory must not execute");
        })
        .expect("managed registration succeeds");
    builder.root::<u8>();
    let application = builder.build().expect("ordinary selected graph needs no policy");
    assert_eq!(*application.context().get::<u8>().expect("selected instance exists"), 1);
}

#[test]
fn test_ordinary_graph_does_not_require_wait_policy() {
    let application = ContainerBuilder::new().build_all().expect("empty graph builds");
    assert_eq!(application.context().state(), ApplicationState::Running);
}

/// Builds a selected asynchronous managed component after an ordinary
/// dependency.
fn async_managed_builder(calls: &Arc<AtomicUsize>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    let observed = Arc::clone(calls);
    builder
        .register_factory::<u8, _>(&[], move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(1))
        })
        .expect("ordinary dependency registration succeeds");
    let observed = Arc::clone(calls);
    builder
        .register_managed_async_factory::<u32, _>(&[Dependency::of::<u8>()], move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Managed::new(Arc::new(7), |_| Ok(()))) })
        })
        .expect("async managed registration succeeds");
    builder.root::<u32>();
    builder
}

#[test]
fn test_missing_policy_precedes_selected_managed_async_preflight() {
    let calls = Arc::new(AtomicUsize::new(0));
    assert!(
        matches!(async_managed_builder(&calls).build(), Err(failure) if matches!(failure.cause(), BuildError::MissingWaitPolicy))
    );
    assert!(
        matches!(async_managed_builder(&calls).build_all(), Err(failure) if matches!(failure.cause(), BuildError::MissingWaitPolicy))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_async_root_build_missing_policy_prevents_every_factory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = Builder::new_current_thread().build().expect("test runtime builds");
    let result = runtime.block_on(async_managed_builder(&calls).build_async());
    assert!(matches!(result, Err(failure) if matches!(failure.cause(), BuildError::MissingWaitPolicy)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_async_all_build_missing_policy_prevents_every_factory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = Builder::new_current_thread().build().expect("test runtime builds");
    let result = runtime.block_on(async_managed_builder(&calls).build_all_async());
    assert!(matches!(result, Err(failure) if matches!(failure.cause(), BuildError::MissingWaitPolicy)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
