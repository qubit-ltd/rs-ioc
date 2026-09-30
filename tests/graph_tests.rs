// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::any::type_name;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ComponentDefinition;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Definition;
use qubit_ioc::DefinitionSource;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::RegistrationError;

struct Root;
struct Good;
struct Bad;
struct CycleRoot;
struct CycleB;
struct CycleC;
struct OptionalRoot(Option<Arc<u8>>);
struct Collector(Vec<Arc<u32>>);

trait Named: Send + Sync {
    fn name(&self) -> &'static str;
}

struct First;
struct Second;
struct OrderRoot;
struct OrderLeft;
struct OrderRight;
struct MissingRoot;
struct FirstRoot;
struct SecondRoot;
struct DeepRoot;
struct Intermediate;
struct SharedBad;
struct AliasConcrete;
struct IsolatedGood;
struct IsolatedBad;

trait AliasService: Send + Sync {}
impl AliasService for AliasConcrete {}

impl Named for First {
    fn name(&self) -> &'static str {
        "first"
    }
}

impl Named for Second {
    fn name(&self) -> &'static str {
        "second"
    }
}

struct Aliases;

impl ComponentDefinition for Aliases {
    fn source() -> DefinitionSource {
        DefinitionSource::new("tests", "graph", "tests/graph_tests.rs", 1, 1, "Aliases")
    }

    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        builder.register_instance(Arc::new(First))?;
        builder.register_instance_with::<dyn Named>(
            Arc::new(First),
            BindingOptions {
                id: Some("first".into()),
                primary: true,
                order: 1,
                ..BindingOptions::default()
            },
        )?;
        builder.register_instance_with::<dyn Named>(
            Arc::new(Second),
            BindingOptions {
                id: Some("second".into()),
                order: 2,
                ..BindingOptions::default()
            },
        )
    }
}

/// Polls a future that is expected to complete on its first poll.
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test future unexpectedly yielded"),
    }
}

#[test]
fn test_factory_failure_in_second_branch_keeps_root_in_path() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<Root, _>(&[Dependency::of::<Good>(), Dependency::of::<Bad>()], |_| {
            Ok(Arc::new(Root))
        })
        .expect("register root");
    builder.register_instance(Arc::new(Good)).expect("register good");
    builder
        .register_factory::<Bad, _>(&[], |_| Err(FactoryError::new(std::io::Error::other("bad"))))
        .expect("register bad");
    builder.root::<Root>();

    let error = builder.build().err().expect("factory must fail");
    match error.cause() {
        BuildError::FactoryFailed { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), type_name::<Root>());
            assert_eq!(path[1].type_name(), type_name::<Bad>());
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn test_async_factory_failure_in_second_branch_keeps_root_in_path() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<Root, _>(&[Dependency::of::<Good>(), Dependency::of::<Bad>()], |_| {
            Ok(Arc::new(Root))
        })
        .expect("register root");
    builder.register_instance(Arc::new(Good)).expect("register good");
    builder
        .register_async_factory::<Bad, _>(&[], |_| {
            Box::pin(async { Err(FactoryError::new(std::io::Error::other("bad"))) })
        })
        .expect("register bad");
    builder.root::<Root>();

    let error = ready(builder.build_async()).err().expect("factory must fail");
    match error.cause() {
        BuildError::FactoryFailed { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), type_name::<Root>());
            assert_eq!(path[1].type_name(), type_name::<Bad>());
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn test_cycle_error_keeps_explicit_root_registered_after_cycle() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<CycleB, _>(&[Dependency::of::<CycleC>()], |_| Ok(Arc::new(CycleB)))
        .expect("register B before cycle root");
    builder
        .register_factory::<CycleC, _>(&[Dependency::of::<CycleB>()], |_| Ok(Arc::new(CycleC)))
        .expect("register C before cycle root");
    builder
        .register_factory::<CycleRoot, _>(&[Dependency::of::<CycleB>()], |_| Ok(Arc::new(CycleRoot)))
        .expect("register cycle root");
    builder.root::<CycleRoot>();

    let error = builder.build().err().expect("cycle must fail");
    match error.cause() {
        BuildError::DependencyCycle { path } => assert_eq!(
            path.iter().map(|key| key.type_name()).collect::<Vec<_>>(),
            vec![
                type_name::<CycleRoot>(),
                type_name::<CycleB>(),
                type_name::<CycleC>(),
                type_name::<CycleB>(),
            ]
        ),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn test_root_factory_failure_restores_a_deep_path_without_recursive_walks() {
    const NODE_COUNT: usize = 10_000;
    let mut builder = ContainerBuilder::new();
    for index in 0..NODE_COUNT {
        let dependencies = if index + 1 == NODE_COUNT {
            Vec::new()
        } else {
            vec![Dependency::with_id::<u32>(&format!("chain.n{}", index + 1))]
        };
        let options = BindingOptions {
            id: Some(format!("chain.n{index}")),
            ..BindingOptions::default()
        };
        builder
            .register_factory_with::<u32, _>(&dependencies, options, move |_| {
                if index + 1 == NODE_COUNT {
                    Err(FactoryError::new(std::io::Error::other("end of chain")))
                } else {
                    Ok(Arc::new(index as u32))
                }
            })
            .expect("register chain binding");
    }
    builder.root_by_id::<u32>("chain.n0").expect("select chain root");

    let error = builder.build().err().expect("last factory must fail");
    match error.cause() {
        BuildError::FactoryFailed { path, .. } => {
            assert_eq!(path.len(), NODE_COUNT);
            assert_eq!(
                path.first().and_then(|key| key.id()).map(|id| id.as_str()),
                Some("chain.n0")
            );
            assert_eq!(
                path.last().and_then(|key| key.id()).map(|id| id.as_str()),
                Some("chain.n9999")
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn test_optional_dependency_is_none_only_when_missing() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<OptionalRoot, _>(&[Dependency::optional::<u8>()], |context| {
            Ok(Arc::new(OptionalRoot(context.try_get::<u8>().expect("valid request"))))
        })
        .expect("register optional root");
    builder.root::<OptionalRoot>();
    let application = builder.build().expect("missing optional is allowed");
    let context = application.context();
    assert!(
        context.get::<OptionalRoot>().expect("root").0.is_none(),
        "an optional dependency with no candidate must resolve to None"
    );
}

#[test]
fn test_optional_dependency_rejects_ambiguity() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<OptionalRoot, _>(&[Dependency::optional::<u8>()], |context| {
            Ok(Arc::new(OptionalRoot(context.try_get::<u8>().expect("valid request"))))
        })
        .expect("register optional root");
    builder
        .register_instance_with(
            Arc::new(1_u8),
            BindingOptions {
                id: Some("one".into()),
                ..BindingOptions::default()
            },
        )
        .expect("one");
    builder
        .register_instance_with(
            Arc::new(2_u8),
            BindingOptions {
                id: Some("two".into()),
                ..BindingOptions::default()
            },
        )
        .expect("two");
    builder.root::<OptionalRoot>();
    assert!(
        matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::AmbiguousBinding { .. })),
        "two candidates for one optional dependency must be reported as ambiguous"
    );
}

#[test]
fn test_collection_dependency_is_stable() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<Collector, _>(&[Dependency::all::<u32>()], |context| {
            Ok(Arc::new(Collector(context.get_all::<u32>().expect("valid collection"))))
        })
        .expect("register collector");
    for (id, order, value) in [("z", 1, 10_u32), ("b", 0, 2_u32), ("a", 0, 1_u32)] {
        builder
            .register_instance_with(
                Arc::new(value),
                BindingOptions {
                    id: Some(id.into()),
                    order,
                    ..BindingOptions::default()
                },
            )
            .expect("register collection item");
    }
    builder.root::<Collector>();
    let application = builder.build().expect("collection dependencies resolve");
    let context = application.context();
    let collector = context.get::<Collector>().expect("collector");
    assert_eq!(collector.0.iter().map(|value| **value).collect::<Vec<_>>(), [1, 2, 10]);
}

#[test]
fn test_unique_primary_selects_interface_alias() {
    let mut builder = ContainerBuilder::new();
    builder.install::<Aliases>().expect("install aliases");
    builder.root::<dyn Named>();
    let application = builder.build().expect("one alias is primary");
    let context = application.context();
    assert_eq!(context.get::<dyn Named>().expect("primary").name(), "first");
}

#[test]
fn test_concrete_binding_does_not_inherit_alias_priority() {
    let mut builder = ContainerBuilder::new();
    builder.install::<Aliases>().expect("install aliases");
    let application = builder
        .build_all()
        .expect("alias priority does not affect concrete binding");
    let context = application.context();
    assert_eq!(context.get::<First>().expect("concrete binding").name(), "first");
}

#[test]
fn test_active_profiles_replace_previous_selection_without_running_factories() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["dev"])
        .expect("select dev")
        .active_profiles(&["prod"])
        .expect("replace with prod");
    for (id, profile) in [
        ("env.default", Some("default")),
        ("env.dev", Some("dev")),
        ("env.prod", Some("prod")),
        ("env.common", None),
    ] {
        let calls = Arc::clone(&calls);
        builder
            .register_factory_with::<u8, _>(
                &[],
                BindingOptions {
                    id: Some(id.into()),
                    profile: profile.map(Into::into),
                    ..BindingOptions::default()
                },
                move |_| {
                    calls.lock().expect("calls").push(id);
                    Ok(Arc::new(1))
                },
            )
            .expect("register profile factory");
    }
    assert!(
        calls.lock().expect("calls").is_empty(),
        "profile selection must be resolved before any factory runs"
    );
    let application = builder.build_all().expect("dev definition is filtered out");
    let context = application.context();
    assert!(
        context.get_by_id::<u8>("env.dev").is_err(),
        "the previously selected dev definition must be replaced by prod"
    );
    assert_eq!(calls.lock().expect("calls").as_slice(), ["env.prod", "env.common"]);
    drop(application);

    let mut defaults = ContainerBuilder::new()
        .active_profiles(&[])
        .expect("default profile only");
    for (id, profile) in [
        ("env.default", Some("default")),
        ("env.dev", Some("dev")),
        ("env.prod", Some("prod")),
        ("env.common", None),
    ] {
        defaults
            .register_instance_with(
                Arc::new(id),
                BindingOptions {
                    id: Some(id.into()),
                    profile: profile.map(Into::into),
                    ..BindingOptions::default()
                },
            )
            .expect("register profile instance");
    }
    let application = defaults
        .build_all()
        .expect("only default and profile-free definitions remain");
    let context = application.context();
    assert!(
        context.get_by_id::<&str>("env.default").is_ok(),
        "the default profile must stay selected without an explicit active profile"
    );
    assert!(
        context.get_by_id::<&str>("env.common").is_ok(),
        "a profile-free definition must survive an empty active profile list"
    );
    assert!(
        context.get_by_id::<&str>("env.dev").is_err(),
        "a non-selected named profile must be filtered out"
    );
    assert!(
        context.get_by_id::<&str>("env.prod").is_err(),
        "a non-selected named profile must be filtered out"
    );
}

#[test]
fn test_factories_run_dependency_first_with_stable_same_level_order() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new();
    let root_events = Arc::clone(&events);
    builder
        .register_factory::<OrderRoot, _>(
            &[Dependency::of::<OrderLeft>(), Dependency::of::<OrderRight>()],
            move |_| {
                root_events.lock().expect("events").push("root");
                Ok(Arc::new(OrderRoot))
            },
        )
        .expect("register root first");
    let right_events = Arc::clone(&events);
    builder
        .register_factory::<OrderRight, _>(&[], move |_| {
            right_events.lock().expect("events").push("right");
            Ok(Arc::new(OrderRight))
        })
        .expect("register right second");
    let left_events = Arc::clone(&events);
    builder
        .register_factory::<OrderLeft, _>(&[], move |_| {
            left_events.lock().expect("events").push("left");
            Ok(Arc::new(OrderLeft))
        })
        .expect("register left third");
    builder.root::<OrderRoot>();
    drop(builder.build().expect("valid graph builds"));
    assert_eq!(events.lock().expect("events").as_slice(), ["right", "left", "root"]);
}

#[test]
fn test_graph_error_prevents_all_factory_side_effects() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&events);
    builder
        .register_factory::<MissingRoot, _>(&[Dependency::of::<Bad>()], move |_| {
            captured.lock().expect("events").push("root");
            Ok(Arc::new(MissingRoot))
        })
        .expect("register root");
    builder.root::<MissingRoot>();
    assert!(
        matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::MissingDependency { .. })),
        "a missing dependency must fail the build with a structured error"
    );
    assert!(
        events.lock().expect("events").is_empty(),
        "a graph error must be detected before any factory side effect"
    );
}

#[test]
fn test_shared_failure_uses_declared_root_order_for_equal_length_paths() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<FirstRoot, _>(&[Dependency::of::<SharedBad>()], |_| Ok(Arc::new(FirstRoot)))
        .expect("first root");
    builder
        .register_factory::<SecondRoot, _>(&[Dependency::of::<SharedBad>()], |_| Ok(Arc::new(SecondRoot)))
        .expect("second root");
    builder
        .register_factory::<SharedBad, _>(&[], |_| Err(FactoryError::new(std::io::Error::other("shared"))))
        .expect("shared failing dependency");
    builder.root::<FirstRoot>();
    builder.root::<SecondRoot>();
    match builder.build().err().expect("shared dependency fails").cause() {
        BuildError::FactoryFailed { path, .. } => assert_eq!(
            path.iter().map(|key| key.type_name()).collect::<Vec<_>>(),
            [type_name::<FirstRoot>(), type_name::<SharedBad>()]
        ),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn test_shared_failure_prefers_shorter_root_path() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<DeepRoot, _>(&[Dependency::of::<Intermediate>()], |_| Ok(Arc::new(DeepRoot)))
        .expect("deep root");
    builder
        .register_factory::<Intermediate, _>(&[Dependency::of::<SharedBad>()], |_| Ok(Arc::new(Intermediate)))
        .expect("intermediate");
    builder
        .register_factory::<SecondRoot, _>(&[Dependency::of::<SharedBad>()], |_| Ok(Arc::new(SecondRoot)))
        .expect("short root");
    builder
        .register_factory::<SharedBad, _>(&[], |_| Err(FactoryError::new(std::io::Error::other("shared"))))
        .expect("shared failing dependency");
    builder.root::<DeepRoot>();
    builder.root::<SecondRoot>();
    match builder.build().err().expect("shared dependency fails").cause() {
        BuildError::FactoryFailed { path, .. } => assert_eq!(
            path.iter().map(|key| key.type_name()).collect::<Vec<_>>(),
            [type_name::<SecondRoot>(), type_name::<SharedBad>()]
        ),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn test_collection_failure_path_identifies_second_ordered_item() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<Collector, _>(&[Dependency::all::<u32>()], |context| {
            Ok(Arc::new(Collector(context.get_all::<u32>().expect("collection"))))
        })
        .expect("collector");
    builder
        .register_instance_with(
            Arc::new(1_u32),
            BindingOptions {
                id: Some("branch.first".into()),
                order: 1,
                ..BindingOptions::default()
            },
        )
        .expect("first branch");
    builder
        .register_factory_with::<u32, _>(
            &[],
            BindingOptions {
                id: Some("branch.second".into()),
                order: 2,
                ..BindingOptions::default()
            },
            |_| Err(FactoryError::new(std::io::Error::other("second branch"))),
        )
        .expect("second branch");
    builder.root::<Collector>();
    match builder.build().err().expect("second item fails").cause() {
        BuildError::FactoryFailed { path, .. } => assert_eq!(
            path.iter()
                .map(|key| key.id().map(|id| id.as_str()))
                .collect::<Vec<_>>(),
            [None, Some("branch.second")]
        ),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn test_trait_alias_root_factory_failure_path_includes_concrete_member() {
    let mut builder = ContainerBuilder::new();
    let definition = Definition::<AliasConcrete>::builder()
        .source(DefinitionSource::new(
            "tests",
            "graph",
            "tests/graph_tests.rs",
            1,
            1,
            "AliasConcrete",
        ))
        .factory(|_| Err(FactoryError::new(std::io::Error::other("concrete failure"))))
        .bind::<dyn AliasService, _>(BindingOptions::default(), |value| value)
        .build()
        .expect("definition with alias");
    builder.register_definition(definition).expect("register definition");
    builder.root::<dyn AliasService>();
    match builder.build().err().expect("concrete factory fails").cause() {
        BuildError::FactoryFailed { path, .. } => assert_eq!(
            path.iter().map(|key| key.type_name()).collect::<Vec<_>>(),
            [type_name::<dyn AliasService>(), type_name::<AliasConcrete>()]
        ),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn test_build_all_isolated_factory_failure_has_single_binding_path() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance(Arc::new(IsolatedGood))
        .expect("good isolated binding");
    builder
        .register_factory::<IsolatedBad, _>(&[], |_| Err(FactoryError::new(std::io::Error::other("isolated"))))
        .expect("bad isolated binding");
    match builder.build_all().err().expect("isolated factory fails").cause() {
        BuildError::FactoryFailed { path, .. } => assert_eq!(
            path.iter().map(|key| key.type_name()).collect::<Vec<_>>(),
            [type_name::<IsolatedBad>()]
        ),
        other => panic!("unexpected error: {other}"),
    }
}
