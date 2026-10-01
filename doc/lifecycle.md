# Managed Application Lifecycle

## Application-wide shutdown deadline

`WaitPolicy::bounded(grace, termination, timer)` retains per-component
deadlines. Use `bounded_with_total(grace, termination, total, timer)` to add an
application-wide deadline. Its timer starts on the first poll of
`ShutdownHandle::wait()` and is reused if that borrowed future is cancelled or
the shutdown mode is upgraded. It does not restart for `abort()`. Empty
applications, `abandon()`, and Drop do not start it.

When the total timer expires, IoC requests abort for every unconfirmed managed
component and records the cause in `ShutdownReport::overall_failure()`. Pending
components appear in `incomplete()`; prior failures and fallbacks remain in the
report. `is_complete()` means termination was confirmed, while `is_success()`
also requires that no overall failure occurred. A deadline cannot interrupt a
synchronous callback, one blocking future poll, or a destructor.

## Typed synchronous factories

`register_injected_factory` and `register_injected_managed_factory` take an
explicit parameter tuple. `Arc<T>` declares a required request,
`Option<Arc<T>>` an optional request, and `Vec<Arc<T>>` a collection request.
Tuples support zero through eight arguments. Repeated identical requests are
deduplicated in the graph while each argument receives its own resolved value.
Use the existing registration methods for IDs, async factories, or larger
signatures.

[中文生命周期说明](lifecycle.zh_CN.md) · [User guide](user_guide.md)

This guide describes `qubit-ioc` 0.3.0. Register managed factories during
assembly, then create each resource inside its factory after graph validation.
Return `Managed<T>` to transfer its value and cleanup actions to the container.
An already-running external resource can use `register_instance(Arc<T>)`; the
application keeps its shutdown responsibility. A factory owns side effects
created before it returns `Managed`, including cancellation of unfinished
construction; use its own RAII guards for those effects.

The executable [managed worker example](../examples/app_lifecycle.rs) starts a
Tokio task inside a factory, sends a one-shot stop signal, awaits its task
handle, and verifies that it exited:

```bash
cargo +1.94.0 run --example app_lifecycle --no-default-features --locked
```

Tokio is an example dependency; the IoC runtime does not depend on it. A worker
that has no separate drain protocol can use its abort callback as a graceful
fallback, which is recorded in `ShutdownReport::fallbacks()`.

## Retain the owner and observe every exit path

`build`, `build_all`, and their async variants return `Application`. Keep this
unique owner until shutdown. Share `application.context().clone()` for
read-only queries; clones can remain alive while the owner shuts down.
`context.state()` exposes Running, ShuttingDown, Closed, or Incomplete. Queries
still return the stored `Arc`s after shutdown starts: lookup does not prove
that the service still accepts work. Components control their own admission.
External clones can also keep object memory alive after resource termination.

A selected managed graph requires an explicit `WaitPolicy`, otherwise build
returns a `BuildFailure` whose cause is `BuildError::MissingWaitPolicy` before
any factory runs. Real managed applications should use `WaitPolicy::bounded`.
The following integration function takes an already registered builder and an
application-provided business operation. It runs inside a Tokio runtime with
time enabled (`tokio` features `rt` and `time`). It observes cleanup on both
startup failure and business failure; normal exit chooses Graceful:

```rust
use std::error::Error;
use std::time::Duration;
use qubit_ioc::{ApplicationContext, ContainerBuilder, ShutdownMode, WaitPolicy};

// The caller registers the application's resources before calling this function.
async fn run<F>(builder: ContainerBuilder, business: F) -> Result<(), Box<dyn Error>>
where
    F: FnOnce(&ApplicationContext) -> Result<(), Box<dyn Error>>,
{
    let builder = builder.wait_policy(WaitPolicy::bounded(
        Duration::from_secs(30),
        Duration::from_secs(5),
        |duration| Box::pin(tokio::time::sleep(duration)),
    ));
    let application = match builder.build_async().await {
        Ok(application) => application,
        Err(mut failure) => {
            eprintln!("build failed: {}", failure.cause());
            if let Some(mut cleanup) = failure.take_cleanup() {
                if let Err(error) = cleanup.wait().await {
                    eprintln!("rollback report: {:?}", error.report());
                }
            }
            let (cause, _) = failure.into_parts();
            return Err(cause.into());
        }
    };
    let context = application.context().clone();
    let result = business(&context);
    let mode = if result.is_ok() { ShutdownMode::Graceful } else { ShutdownMode::Immediate };
    let mut shutdown = application.begin_shutdown(mode);
    let cleanup_result = shutdown.wait().await;
    if let Err(error) = result {
        if let Err(cleanup_error) = cleanup_result {
            eprintln!("shutdown report: {:?}", cleanup_error.report());
        }
        return Err(error);
    }
    cleanup_result?;
    Ok(())
}
```

The business operation returns its error to the owner before the owner is
consumed. Async business operations need the same structure: retain the owner,
await business work into a result, select the mode, and await shutdown before
returning that result. If both business and cleanup fail, this fragment logs
the cleanup report and returns the business error; applications can retain both
in their own error type.

Both synchronous and asynchronous builds request abort for all transferred
managed resources and immediately return `BuildFailure` when a later factory
returns an error. `cause()` preserves the original error, sources and path;
`take_cleanup()` transfers cleanup ownership once. `into_parts()` is an
alternative that returns the cause and optional handle together. Graph or
preflight failure has no constructed resources and no cleanup handle. Neither
async build nor dropping `BuildFailure` waits for rollback. A factory unwind
panic still propagates; cancellation or unwinding requests abort without wait.

## Request contracts and shutdown ordering

`Managed::new(value, abort)` requires a synchronous, non-blocking cancellation
request. It must not join, block on a future or condition variable, execute
business handlers, or perform unbounded I/O. `.with_graceful_stop(request)`
requests draining of this component: close its own admission without closing
its dependencies. `.with_wait(wait)` confirms resource termination. Without a
wait action, a successful stop callback must mean termination is already
complete; a ready future must not stand in for an unfinished worker.

- `application.begin_shutdown(ShutdownMode::Graceful)` transfers ownership and
  publishes ShuttingDown. The first request runs when `wait()` is polled.
  In reverse construction order, each consumer receives its graceful request
  and is awaited before its dependency receives a shutdown request. A component
  without graceful support uses abort and appears in `fallbacks()`.
- `application.begin_shutdown(ShutdownMode::Immediate)` requests every abort
  in reverse construction order before returning, then explicit `wait()` waits
  in that order. Later callback requests continue after an error or unwind panic.
  Failure and cancellation use this abort path.

Only successful concrete managed definitions create lifecycle entries; aliases
do not duplicate them. Registration and graph validation never run factories
or alias projectors. Construction and shutdown are serial.

## Deadlines, cancellation, and reports

A bounded policy has independent per-component grace and termination budgets.
Graceful timeout records a failure, requests abort for that component, and
keeps its existing pending wait under the termination budget. Another timeout
marks it incomplete and proceeds to later entries. A graceful request failure
also falls back to abort; wait errors or panics cannot recreate a consumed
one-shot wait to prove termination. A termination budget failure degrades the
consumer-before-dependency guarantee for that unfinished consumer.

The application must drive the supplied timer. Deadlines constrain futures
that yield Pending; they cannot interrupt blocking callbacks, blocking
`Future::poll`, destructors, or `panic = "abort"`. Timer creation/poll unwind
panics are recorded as Deadline failures and do not stop later entries.
`WaitPolicy::unbounded()` explicitly allows indefinite waiting and is useful
for controlled tests; it provides no finite termination guarantee.

Cancelling the borrowing `wait()` future retains the active wait, deadline,
phase and budget in the handle. Calling `wait()` again on that handle resumes
without recreating actions or restarting the deadline. `abort()` upgrades all
unfinished entries to Immediate behavior without repeating abort requests;
it retains the pending wait and does not reset an existing termination budget.
`pending()` lists entries whose termination remains unconfirmed.

`wait()` returns a `ShutdownReport`, or `ShutdownError` carrying its report.
`failures()` retains binding keys, sources, phases and original cleanup errors.
`is_complete()` only means `incomplete()` is empty; `is_success()` also requires
no failures. A complete report may still contain callback errors. Fallbacks
alone are not failures. Repeated completed waits return the same observations
without repeating actions. `mode()` retains the originally requested mode,
even when `abort()` upgrades the handle.

Incomplete means termination was not confirmed. It does not mean the runtime
killed the resource; it may continue running and retain external handles.
Applications must inspect the report and apply the resource's own recovery
protocol rather than assuming timeout removed all work.

## Drop and deliberate abandonment

`Application`, untransferred `Managed`, and `ShutdownHandle` Drop request
best-effort abort for unfinished owned entries without creating or polling
waits. Drop cannot report success or cleanup errors. A query context's Drop
does not request shutdown. `shutdown.abandon()` explicitly requests remaining
aborts and returns a report marking unconfirmed termination incomplete; it
starts no new waits. Dropping a Graceful handle before its first poll requests
abort, so creating that handle alone does not perform graceful draining.
The owner and cleanup handle are not cloneable; `must_use` is a reminder to
observe their lifecycle, not a promise that Drop finishes it.

## EventBus adapters

For `qubit-event-bus` 0.18.0, even synchronous
`EventBus::shutdown(Immediate)` waits for workers and providers. Never call it
from a managed abort or graceful request callback. Use
`EventBus::request_shutdown(mode)` there and retain its `EventBusShutdown`
ticket; await `ticket.wait_async()` inside the managed wait callback. A graceful
request closes admission and drains; a later Immediate request strengthens that
attempt. The ticket tracks that attempt's generation. Dropping a ticket or
cancelling its async observation does not cancel background shutdown.
`ticket.wait(timeout)` bounds a synchronous observer, while IoC `WaitPolicy`
bounds the async wait; neither forcibly kills a provider. A synchronous
`shutdown` wrapped in `spawn_blocking` does not meet the abort request contract.
The [downstream consumer](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/README.md)
shows the request/ticket adapter and the application's exit responsibility.

## Migration from 0.2 to 0.3

These are breaking changes; old calls below are historical migration inputs.
Runtime and macro packages must both use 0.3.0. There is no old lifecycle or
code-generation compatibility layer.

| Previous call or behavior | 0.3 replacement and responsibility |
| --- | --- |
| `ApplicationContext::builder()` | Use `Application::builder()` or `ContainerBuilder::new()`. |
| Build returns `ApplicationContext` | Build returns `Application`; query through `application.context()`, clone that handle for sharing. |
| `Arc::try_unwrap(context)` before shutdown | Retain the unique application owner and call its shutdown directly; query clones can remain alive. |
| `begin_shutdown()` stops all before waiting | Choose `begin_shutdown(ShutdownMode::Graceful)` for normal exit or `Immediate` for failure; Graceful requests start on the first wait poll. |
| Async build waits for rollback before returning error | `BuildFailure` immediately delivers `cause()` and optional `take_cleanup()` / `into_parts()` owner; explicitly drive cleanup `wait()`. `BuildError::CleanupFailed` is removed; cleanup errors belong to the shutdown report. |
| Context / `Managed` Drop performs no cleanup | Query context still has no shutdown responsibility; owner, untransferred `Managed`, and handle Drop request abort, never wait. |
| Hidden `codegen_v1::DefinitionDraft` | Use public `Definition::builder()` and `register_definition`; macros use this core too. Only configuration diagnostics and generated-code glue remain hidden. |
| `EventBus::shutdown` used as a stop callback | Use non-blocking `request_shutdown`, retain the ticket, and await `wait_async()` in the wait callback. |
| Collections sorted on each query | Publication pre-sorts immutable per-type indexes by order, ID, source and registration position; queries reuse the order. |

Collection injection and post-build collections keep ascending order, ID,
source location and stable registration ties. This is independent of
construction/cleanup order. Exact-key and per-type indexes remove specific
repeated candidate/member scans; the whole graph algorithm is not promised to
be strictly linear, and no fixed timing or production speedup is guaranteed.

Downstream lifecycle verification keeps separate
[historical](../tests/fixtures/application_consumer/Cargo.toml) and
[current](../tests/fixtures/application_consumer_current/Cargo.toml) locked
snapshots. Historical verification covers that pin's code. The current
refactor snapshot uses local reviewed paths until external changes have been
committed and pinned; it does not claim an unpublished external SHA exists.
See the [current design](complete-design.md) for these boundaries.
