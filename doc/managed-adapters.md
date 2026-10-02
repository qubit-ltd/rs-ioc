# Managed resource adapters

[中文指南](managed-adapters.zh_CN.md) · [Lifecycle guide](lifecycle.md) · [User guide](user_guide.md)

This guide covers `qubit-ioc` 0.3.0. Use a managed factory when the application
creates a resource and must confirm its termination before stopping its
dependencies. Register the factory during assembly; create the resource only
when the selected graph is built. Keep the returned `Application` as the
lifecycle owner, configure a driven, bounded `WaitPolicy`, and await its
shutdown handle before dropping the runtime that drives the resource.

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
for each consumer before stopping its dependencies. During cancellation, Drop,
or a failed build, IoC requests abort but cannot wait automatically; if a
failed build returns a cleanup handle, the application must drive it.

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

## Preserve a shutdown ticket through cancellation

Some resources return a shutdown ticket when asked to stop. Store that ticket
in adapter state, and move it into the managed `wait` future. The ticket must
remain owned by the future while it is pending. `ShutdownHandle::wait()` borrows
the handle: dropping that borrowing future keeps the active wait and deadline
in the handle, so polling `wait()` again resumes the same observation. Dropping
the handle itself abandons observation and requests remaining aborts; it does
not prove termination.

The [EventBus integration fixture](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/src/managed_event_bus.rs)
shows the request and ticket pattern. Its abort callback calls
`EventBus::request_shutdown(Immediate)`, retains the returned
`EventBusShutdown`, and its wait callback awaits `ticket.wait_async()`.
`EventBus::shutdown(Immediate)` waits synchronously for workers and providers,
so it does not satisfy the nonblocking abort contract. The ticket identifies
the shutdown attempt; cancelling observation or dropping the ticket does not
cancel background shutdown. This source is an integration fixture that checks
a cross crate contract, not evidence of production adoption.

## Graceful requests and Immediate escalation

Add `.with_graceful_stop(request)` when a resource can first stop accepting new
work and drain current work. The request must only affect its own admission and
must not close its dependencies. `Graceful` begins on the first poll of the
shutdown handle. IoC waits for each consumer before moving to its dependencies.
If the graceful request fails or its grace budget expires, IoC requests abort.
Calling `ShutdownHandle::abort()` upgrades unfinished entries to Immediate
without recreating an active wait or restarting its deadline.

For EventBus, issue `request_shutdown(Graceful { timeout })` in the graceful
callback, retain its ticket, and issue `request_shutdown(Immediate)` if an
upgrade is needed. The later request strengthens the same shutdown attempt;
the retained wait observes its completion. Missing graceful support is an
explicit fallback reported by `ShutdownReport::fallbacks()`.

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
