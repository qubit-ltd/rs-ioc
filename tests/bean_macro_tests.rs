// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "macros")]

use std::error::Error;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

#[cfg(feature = "config")]
use qubit_config::Config;
use qubit_ioc::BuildError;
use qubit_ioc::ComponentDefinition;
use qubit_ioc::Configuration;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::bean;

#[derive(Debug, PartialEq)]
struct BareValue(u32);
#[derive(Debug, PartialEq)]
struct SharedValue(u32);
#[derive(Debug, PartialEq)]
struct FallibleValue(u32);
#[derive(Debug, PartialEq)]
struct FallibleSharedValue(u32);
#[derive(Debug, PartialEq)]
struct AsyncBareValue(u32);
#[derive(Debug, PartialEq)]
struct AsyncSharedValue(u32);
#[derive(Debug, PartialEq)]
struct AsyncFallibleValue(u32);
#[derive(Debug, PartialEq)]
struct AsyncFallibleSharedValue(u32);

#[derive(Debug, thiserror::Error)]
#[error("bean factory failure")]
struct BeanFailure;

#[bean]
fn bare_value() -> BareValue {
    BareValue(11)
}

#[bean]
fn shared_value() -> Arc<SharedValue> {
    Arc::new(SharedValue(12))
}

#[bean]
fn fallible_value() -> Result<FallibleValue, BeanFailure> {
    Ok(FallibleValue(13))
}

#[bean]
fn fallible_shared_value() -> Result<Arc<FallibleSharedValue>, BeanFailure> {
    Ok(Arc::new(FallibleSharedValue(14)))
}

#[bean]
async fn async_bare_value() -> AsyncBareValue {
    AsyncBareValue(15)
}

#[bean]
async fn async_shared_value() -> Arc<AsyncSharedValue> {
    Arc::new(AsyncSharedValue(16))
}

#[bean]
async fn async_fallible_value() -> Result<AsyncFallibleValue, BeanFailure> {
    Ok(AsyncFallibleValue(17))
}

#[bean]
async fn async_fallible_shared_value() -> Result<Arc<AsyncFallibleSharedValue>, BeanFailure> {
    Ok(Arc::new(AsyncFallibleSharedValue(18)))
}

/// Polls a future known to finish without an executor or external runtime.
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test factories unexpectedly yielded"),
    }
}

#[test]
fn test_bean_keeps_plain_functions_callable_and_supports_all_output_shapes() {
    assert_eq!(bare_value(), BareValue(11));
    assert_eq!(*shared_value(), SharedValue(12));
    assert_eq!(fallible_value().expect("fallible function succeeds"), FallibleValue(13));
    assert_eq!(
        *fallible_shared_value().expect("fallible function succeeds"),
        FallibleSharedValue(14)
    );
    assert_eq!(ready(async_bare_value()), AsyncBareValue(15));
    assert_eq!(*ready(async_shared_value()), AsyncSharedValue(16));
    assert_eq!(
        ready(async_fallible_value()).expect("async function succeeds"),
        AsyncFallibleValue(17)
    );
    assert_eq!(
        *ready(async_fallible_shared_value()).expect("async function succeeds"),
        AsyncFallibleSharedValue(18)
    );

    let mut builder = ContainerBuilder::new();
    builder.install::<BareValueBean>().expect("install bare factory");
    builder.install::<SharedValueBean>().expect("install shared factory");
    builder
        .install::<FallibleValueBean>()
        .expect("install fallible factory");
    builder
        .install::<FallibleSharedValueBean>()
        .expect("install fallible shared factory");
    builder
        .install::<AsyncBareValueBean>()
        .expect("install async bare factory");
    builder
        .install::<AsyncSharedValueBean>()
        .expect("install async shared factory");
    builder
        .install::<AsyncFallibleValueBean>()
        .expect("install async fallible factory");
    builder
        .install::<AsyncFallibleSharedValueBean>()
        .expect("install async fallible shared factory");
    assert!(matches!(builder.build_all(), Err(BuildError::AsyncRequired { .. })));

    let mut builder = ContainerBuilder::new();
    builder.install::<BareValueBean>().expect("install bare factory");
    builder.install::<SharedValueBean>().expect("install shared factory");
    builder
        .install::<FallibleValueBean>()
        .expect("install fallible factory");
    builder
        .install::<FallibleSharedValueBean>()
        .expect("install fallible shared factory");
    builder
        .install::<AsyncBareValueBean>()
        .expect("install async bare factory");
    builder
        .install::<AsyncSharedValueBean>()
        .expect("install async shared factory");
    builder
        .install::<AsyncFallibleValueBean>()
        .expect("install async fallible factory");
    builder
        .install::<AsyncFallibleSharedValueBean>()
        .expect("install async fallible shared factory");
    let context = ready(builder.build_all_async()).expect("all factory shapes build");
    assert_eq!(*context.get::<BareValue>().expect("bare value"), BareValue(11));
    assert_eq!(*context.get::<SharedValue>().expect("shared value"), SharedValue(12));
    assert_eq!(
        *context.get::<FallibleValue>().expect("fallible value"),
        FallibleValue(13)
    );
    assert_eq!(
        *context.get::<FallibleSharedValue>().expect("fallible shared value"),
        FallibleSharedValue(14)
    );
    assert_eq!(
        *context.get::<AsyncBareValue>().expect("async bare value"),
        AsyncBareValue(15)
    );
    assert_eq!(
        *context.get::<AsyncSharedValue>().expect("async shared value"),
        AsyncSharedValue(16)
    );
    assert_eq!(
        *context.get::<AsyncFallibleValue>().expect("async fallible value"),
        AsyncFallibleValue(17)
    );
    assert_eq!(
        *context
            .get::<AsyncFallibleSharedValue>()
            .expect("async fallible shared value"),
        AsyncFallibleSharedValue(18)
    );
}

trait ManagedGreeting: Send + Sync {
    fn message(&self) -> &'static str;
}

struct ManagedGreetingWorker;

impl ManagedGreeting for ManagedGreetingWorker {
    fn message(&self) -> &'static str {
        "managed hello"
    }
}

static MANAGED_GREETING_STOPS: AtomicUsize = AtomicUsize::new(0);

#[bean(marker = ManagedGreetingBean, bind = dyn ManagedGreeting, profile = "managed_test")]
fn managed_greeting() -> Managed<ManagedGreetingWorker> {
    Managed::new(Arc::new(ManagedGreetingWorker), |_| {
        MANAGED_GREETING_STOPS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
}

struct AsyncManagedValue;

static ASYNC_MANAGED_STOPS: AtomicUsize = AtomicUsize::new(0);
static ASYNC_MANAGED_WAITS: AtomicUsize = AtomicUsize::new(0);

#[bean(marker = AsyncManagedValueBean, profile = "managed_test")]
async fn async_managed_value() -> Result<Managed<AsyncManagedValue>, BeanFailure> {
    Ok(Managed::new(Arc::new(AsyncManagedValue), |_| {
        ASYNC_MANAGED_STOPS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .with_wait(|_| {
        Box::pin(async {
            ASYNC_MANAGED_WAITS.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }))
}

struct FailedManagedValue;

#[bean(marker = FailedManagedValueBean, profile = "managed_failure")]
fn failed_managed_value() -> Result<Managed<FailedManagedValue>, BeanFailure> {
    Err(BeanFailure)
}

#[test]
fn test_sync_managed_bean_projects_one_instance_and_stops_once() {
    MANAGED_GREETING_STOPS.store(0, Ordering::SeqCst);
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["managed_test"])
        .expect("active managed test profile");
    builder.install::<ManagedGreetingBean>().expect("install managed bean");
    let context = builder.build_all().expect("sync managed bean builds");
    let concrete = context.get::<ManagedGreetingWorker>().expect("concrete worker");
    let alias = context.get::<dyn ManagedGreeting>().expect("trait alias");
    assert_eq!(alias.message(), "managed hello");
    assert_eq!(Arc::as_ptr(&concrete) as *const (), Arc::as_ptr(&alias) as *const ());
    let mut shutdown = context.begin_shutdown();
    ready(shutdown.wait()).expect("managed shutdown succeeds");
    assert_eq!(MANAGED_GREETING_STOPS.load(Ordering::SeqCst), 1);
}

#[test]
fn test_async_managed_bean_waits_and_build_failure_runs_cleanup() {
    ASYNC_MANAGED_STOPS.store(0, Ordering::SeqCst);
    ASYNC_MANAGED_WAITS.store(0, Ordering::SeqCst);
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["managed_test"])
        .expect("active managed test profile");
    builder
        .install::<AsyncManagedValueBean>()
        .expect("install async managed bean");
    let context = ready(builder.build_all_async()).expect("async managed bean builds");
    assert!(context.get::<AsyncManagedValue>().is_ok());
    let mut shutdown = context.begin_shutdown();
    ready(shutdown.wait()).expect("async managed shutdown succeeds");
    assert_eq!(ASYNC_MANAGED_STOPS.load(Ordering::SeqCst), 1);
    assert_eq!(ASYNC_MANAGED_WAITS.load(Ordering::SeqCst), 1);

    ASYNC_MANAGED_STOPS.store(0, Ordering::SeqCst);
    ASYNC_MANAGED_WAITS.store(0, Ordering::SeqCst);
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["managed_test"])
        .expect("active managed test profile");
    builder
        .install::<AsyncManagedValueBean>()
        .expect("install async managed bean");
    builder
        .register_async_factory::<u64, _>(&[], |_| Box::pin(async { Err(FactoryError::new(BeanFailure)) }))
        .expect("install failing async factory");
    assert!(matches!(
        ready(builder.build_all_async()),
        Err(BuildError::FactoryFailed { .. })
    ));
    assert_eq!(ASYNC_MANAGED_STOPS.load(Ordering::SeqCst), 1);
    assert_eq!(ASYNC_MANAGED_WAITS.load(Ordering::SeqCst), 1);
}

#[test]
fn test_fallible_managed_bean_preserves_error_source() {
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["managed_failure"])
        .expect("active managed failure profile");
    builder
        .install::<FailedManagedValueBean>()
        .expect("install fallible managed bean");
    let error = match builder.build_all() {
        Ok(_) => panic!("fallible managed bean must fail"),
        Err(error) => error,
    };
    let factory_error = error.source().expect("build error source");
    assert!(factory_error.source().expect("bean error source").is::<BeanFailure>());
}

#[derive(Debug)]
struct NamedValue(u32);
#[derive(Debug)]
struct NamedConsumer(Arc<NamedValue>);
#[derive(Debug)]
struct PairValue(u32);

#[bean(id = "example.bean.left")]
fn named_left() -> NamedValue {
    NamedValue(21)
}

#[bean(id = "example.bean.right")]
fn named_right() -> NamedValue {
    NamedValue(22)
}

#[bean(marker = CustomConsumerMarker)]
fn named_consumer(#[inject(id = "example.bean.right")] value: Arc<NamedValue>) -> NamedConsumer {
    NamedConsumer(value)
}

#[bean]
fn repeated_request(
    #[inject(id = "example.bean.right")] first: Arc<NamedValue>,
    #[inject(id = "example.bean.right")] second: Arc<NamedValue>,
) -> PairValue {
    PairValue(first.0 + second.0)
}

#[test]
fn test_bean_inject_id_and_custom_marker() {
    assert_eq!(named_consumer(Arc::new(NamedValue(23))).0.0, 23);
    let mut builder = ContainerBuilder::new();
    builder.install::<NamedLeftBean>().expect("install left binding");
    builder.install::<NamedRightBean>().expect("install right binding");
    builder
        .install::<CustomConsumerMarker>()
        .expect("install custom marker");
    builder
        .install::<RepeatedRequestBean>()
        .expect("install repeated-request bean");
    let context = builder.build_all().expect("exact ID resolves consumer");
    assert_eq!(context.get::<NamedConsumer>().expect("consumer").0.0, 22);
    assert_eq!(context.get::<PairValue>().expect("repeated request").0, 44);
}

#[derive(Debug)]
struct FailingValue;

#[bean(profile = "failure")]
fn failing_value() -> Result<FailingValue, BeanFailure> {
    Err(BeanFailure)
}

#[test]
fn test_bean_preserves_factory_error_source() {
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["failure"])
        .expect("active failure profile");
    builder.install::<FailingValueBean>().expect("install failing factory");
    let error = match builder.build_all() {
        Ok(_) => panic!("failing bean must fail construction"),
        Err(error) => error,
    };
    let source = error.source().expect("factory wrapper");
    assert!(source.source().expect("domain failure").is::<BeanFailure>());
}

trait Greeting: Send + Sync {
    fn message(&self) -> &'static str;
}

#[derive(Debug)]
struct GreetingImpl;

impl Greeting for GreetingImpl {
    fn message(&self) -> &'static str {
        "hello"
    }
}

#[bean(bind = dyn Greeting, primary, order = 2)]
fn greeting() -> GreetingImpl {
    GreetingImpl
}

#[test]
fn test_bean_bind_projects_shared_instance_to_trait() {
    let mut builder = ContainerBuilder::new();
    builder.install::<GreetingBean>().expect("install trait binding");
    let context = builder.build_all().expect("trait binding builds");
    let concrete = context.get::<GreetingImpl>().expect("concrete binding");
    let alias = context.get::<dyn Greeting>().expect("trait binding");
    assert_eq!(alias.message(), "hello");
    assert_eq!(Arc::as_ptr(&concrete) as *const (), Arc::as_ptr(&alias) as *const ());
}

type AliasValue = String;

#[bean(type = String)]
fn alias_value() -> AliasValue {
    "aliased".to_owned()
}

#[test]
fn test_bean_explicit_type_resolves_return_alias() {
    let mut builder = ContainerBuilder::new();
    builder
        .install::<AliasValueBean>()
        .expect("install alias return factory");
    let context = builder.build_all().expect("alias return builds");
    assert_eq!(context.get::<String>().expect("resolved String").as_str(), "aliased");
}

#[cfg(feature = "config")]
#[derive(Debug)]
struct ConfiguredPort(u16);

#[cfg(feature = "config")]
#[bean(profile = "configtest")]
fn configured_port(#[value("service.port")] port: u16) -> ConfiguredPort {
    ConfiguredPort(port)
}

#[cfg(feature = "config")]
#[test]
fn test_bean_value_reads_declared_config_snapshot() {
    let mut config = Config::new();
    config.set("service.port", 8140).expect("set configured port");
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["configtest"])
        .expect("active configuration profile")
        .with_config(config)
        .expect("stage configuration");
    builder.install::<ConfiguredPortBean>().expect("install value bean");
    let context = builder.build_all().expect("value bean builds");
    assert_eq!(context.get::<ConfiguredPort>().expect("configured port").0, 8140);
}

static GROUP_ORDER: AtomicUsize = AtomicUsize::new(0);

#[Configuration(profile = "prod")]
mod grouped {
    use qubit_ioc::bean;

    use super::GROUP_ORDER;
    use super::Ordering;

    #[derive(Debug)]
    pub struct First(pub usize);
    #[derive(Debug)]
    pub struct Second(pub usize);
    #[derive(Debug)]
    pub struct DefaultOnly;
    #[cfg(any())]
    pub struct Disabled;

    #[bean(id = "group.first")]
    pub fn first() -> First {
        First(GROUP_ORDER.fetch_add(1, Ordering::SeqCst))
    }

    #[bean]
    pub fn second() -> Second {
        Second(GROUP_ORDER.fetch_add(1, Ordering::SeqCst))
    }

    #[bean(profile = "default")]
    pub fn default_only() -> DefaultOnly {
        DefaultOnly
    }

    #[cfg(any())]
    #[bean]
    pub fn disabled() -> Disabled {
        Disabled
    }
}

#[test]
fn test_configuration_applies_default_profile_and_installs_in_source_order() {
    let mut builder = ContainerBuilder::new().active_profiles(&["prod"]).expect("active prod");
    grouped::register_ioc(&mut builder).expect("register group in order");
    let context = builder.build_all().expect("group builds");
    let first = context.get::<grouped::First>().expect("first").0;
    let second = context.get::<grouped::Second>().expect("second").0;
    assert!(first < second, "manual group registration must preserve source order");
    assert!(
        context
            .try_get::<grouped::DefaultOnly>()
            .expect("default-only lookup")
            .is_none()
    );

    let mut builder = ContainerBuilder::new();
    grouped::register_ioc(&mut builder).expect("register default group");
    let context = builder.build_all().expect("default group builds");
    assert!(context.try_get::<grouped::First>().expect("prod-only lookup").is_none());
    assert!(
        context
            .try_get::<grouped::Second>()
            .expect("prod-only lookup")
            .is_none()
    );
    assert!(context.get::<grouped::DefaultOnly>().is_ok());
}

#[test]
fn test_configuration_group_installs_beans_and_source_matches_marker() {
    let mut builder = ContainerBuilder::new().active_profiles(&["prod"]).expect("active prod");
    grouped::register_ioc(&mut builder).expect("install configuration beans");
    let context = ready(builder.build_all_async()).expect("configuration group builds");
    assert!(context.get::<grouped::First>().is_ok());
    assert!(context.get::<grouped::Second>().is_ok());
    assert!(
        context
            .try_get::<grouped::DefaultOnly>()
            .expect("inactive override")
            .is_none()
    );
    let source = <grouped::FirstBean as ComponentDefinition>::source();
    assert_eq!(source.item, "first");
}

#[test]
fn test_configuration_group_and_explicit_install_report_duplicate() {
    let mut builder = ContainerBuilder::new().active_profiles(&["prod"]).expect("active prod");
    grouped::register_ioc(&mut builder).expect("manual group install");
    builder.install::<grouped::FirstBean>().expect("duplicate is staged");
    let error = match ready(builder.build_all_async()) {
        Ok(_) => panic!("manual plus linked bean must be duplicate"),
        Err(error) => error,
    };
    assert!(matches!(error, BuildError::DuplicateBinding { .. }));
}
