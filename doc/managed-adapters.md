# Managed resource adapters

[中文指南](managed-adapters.zh_CN.md) · [Lifecycle guide](lifecycle.md) · [User guide](user_guide.md)

This guide covers `qubit-ioc` 0.3.0. Use a managed factory when the application
creates a resource and must confirm its termination before stopping its
dependencies. Register the factory during assembly; create the resource only
when the selected graph is built. Keep the returned `Application` as the
lifecycle owner, configure a driven, bounded `WaitPolicy`, and await its
shutdown handle before dropping the runtime that drives the resource.

## When to create the resource

Registration stages a factory definition; it does not create the managed
resource. IoC validates the selected dependency graph before invoking selected
factories, so create resources that need this protection inside the factory.
After a factory returns `Managed<T>`, IoC owns its lifecycle and can request
abort and observe termination during rollback or shutdown. Before returning,
the factory remains responsible for cleaning up any partial side effects it
created if construction fails or is cancelled. Side effects outside a returned
`Managed<T>` remain the factory's or application's responsibility.

Abort and graceful callbacks issue nonblocking requests. Use `wait` to confirm
termination, or transfer a request ticket to its matching wait callback when
the resource represents shutdown with a ticket. Keep the `Application` and the
runtime that drives waits alive until `ShutdownHandle::wait()` completes; a
bounded `WaitPolicy` also requires the application's executor to drive its
timer.

## Choose how termination is confirmed

For a resource whose stop method finishes termination before it returns,
construct `Managed::synchronous(value, stop)`. A successful `stop` return is
the termination confirmation. Do not use it for a method that only sends a
stop signal. The callback runs synchronously, including on rollback and Drop,
so it must have a bounded execution time.

```rust
use std::sync::Arc;
use qubit_ioc::Managed;

// Worker::stop_and_join is supplied by the application. It returns only
// after the worker has exited, and maps its own error to CleanupError.
let managed = Managed::synchronous(Arc::clone(&worker), |worker| {
    worker.stop_and_join().map_err(qubit_ioc::CleanupError::new)
});
```

For a background worker, construct `Managed::asynchronous(value, abort, wait)`
at creation time. `abort` is a synchronous, nonblocking request; `wait` returns
an owned future that confirms termination. A ready future is valid only after
the resource has actually terminated. During an explicit shutdown, IoC waits
for each consumer before stopping its dependencies. During cancellation or
Drop, IoC requests abort without waiting. A failed build can be observed with
`BuildFailure::wait_cleanup(&mut self).await`, which awaits its optional
cleanup handle while the same `BuildFailure` retains the original cause. If
the wait is cancelled, calling it again resumes cleanup observation.

```rust
use std::sync::Arc;
use qubit_ioc::Managed;

// Worker::request_stop only signals the worker; Worker::join asynchronously
// confirms that it has exited. Both operations are application code.
let managed = Managed::asynchronous(
    Arc::clone(&worker),
    |worker| worker.request_stop().map_err(qubit_ioc::CleanupError::new),
    |worker| Box::pin(async move {
        worker.join().await.map_err(qubit_ioc::CleanupError::new)
    }),
);
```

These are integration fragments: the application supplies `Worker` and its
methods. For a runnable worker, see [`app_lifecycle`](../examples/app_lifecycle.rs).
An adapter that omits the termination wait for an asynchronous resource can
report successful shutdown while that resource still runs. It may then close
dependencies that the worker still uses. `ShutdownReport::incomplete()` means
termination was not confirmed; it does not mean the worker was killed.

## Consume a shutdown ticket

If a stop request returns a ticket, use
`Managed::asynchronous_with_ticket(value, abort, wait)`. Use
`Managed::asynchronous_with_graceful_ticket(value, abort, graceful, wait)`
when both requests return a ticket. The adapter stores an unobserved ticket
and transfers it to the wait future. It does not run a request callback or
poll the wait future while holding its internal lock. For EventBus 0.20, the
complete adapter is:

```rust
use std::sync::Arc;
use std::time::Duration;
use qubit_event_bus::EventBus;
use qubit_event_bus::spi::ShutdownMode as BusShutdownMode;
use qubit_ioc::{CleanupError, Managed};

fn managed_event_bus(bus: Arc<EventBus>) -> Managed<EventBus> {
    Managed::asynchronous_with_graceful_ticket(
        bus,
        |bus| bus.request_shutdown(BusShutdownMode::Immediate)
            .map_err(CleanupError::new),
        |bus| bus.request_shutdown(BusShutdownMode::Graceful {
            timeout: Duration::from_secs(30),
        }).map_err(CleanupError::new),
        |_, ticket| Box::pin(async move {
            ticket.wait_async().await.map(|_| ()).map_err(CleanupError::new)
        }),
    )
}
```

`EventBus::shutdown(Immediate)` waits synchronously for workers and providers,
so it does not satisfy the nonblocking abort contract. EventBus's ticket
identifies a shutdown generation; dropping it does not cancel background
shutdown. **That Drop behavior is required by these ticket constructors:** an
unused graceful or Immediate ticket can be discarded during an upgrade. For a
resource whose ticket Drop cancels shutdown, use lower-level
`Managed::asynchronous` and manage observation according to that resource's
contract. The [EventBus integration fixture](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/src/managed_event_bus.rs)
checks this adapter as a cross crate contract, not as production adoption.

`ShutdownHandle::wait()` borrows the handle. Cancelling that borrowing future
retains the active wait and deadline; calling `wait()` again resumes the same
ticket observation. Dropping the handle itself requests remaining aborts and
does not prove termination.

## Graceful requests and Immediate escalation

Choose graceful behavior when constructing a managed resource. Use
`Managed::synchronous_with_graceful(value, stop, graceful)` or
`Managed::asynchronous_with_graceful(value, abort, graceful, wait)` for
callbacks that do not produce a ticket. The ticket constructor with
`graceful` already binds its request to the matching wait callback. These
constructors keep the graceful callback fixed for the lifetime of the managed
value. A graceful request must only affect its own admission and must not close
dependencies. `Graceful` begins on the first poll of `ShutdownHandle::wait()`.
IoC waits for each consumer before moving to its dependencies.
If the graceful request fails or its grace budget expires, IoC requests abort.
Calling `ShutdownHandle::abort()` upgrades unfinished entries to Immediate
without recreating an active wait or restarting its deadline.

For EventBus, the graceful callback issues `request_shutdown(Graceful {
timeout })`; Immediate upgrade issues `request_shutdown(Immediate)` for the
same shutdown generation. If wait has started, the adapter preserves that
future and discards the new ticket. If wait has not started, the new ticket
replaces the pending graceful ticket. Missing graceful support is an explicit
fallback reported by `ShutdownReport::fallbacks()`.

The [ExecutionServices integration fixture](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/src/managed_execution_services.rs)
separates `stop()` for immediate cancellation, `shutdown()` for a graceful
request, and `await_termination()` for confirmation. It is also an integration
fixture, not production adoption evidence. The caller must keep its Tokio
runtime alive until the termination wait finishes; dropping the runtime first
can prevent the wait from completing.

## Boundaries and diagnosis

A bounded `WaitPolicy` limits yielding wait futures, provided the application
continues to drive its timer. It cannot interrupt a blocking stop callback, a
blocking `Future::poll`, a destructor, or `panic = "abort"`. Avoid blocking in
abort and graceful callbacks; for a background worker, signal in the callback
and confirm completion in `wait`. Inspect the returned `ShutdownReport` for
failed phases, graceful fallbacks, and unconfirmed entries. After a timeout,
use the resource's own recovery protocol instead of assuming it has exited.
