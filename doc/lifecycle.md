# Managed Application Lifecycle

[中文生命周期说明](lifecycle.zh_CN.md) · [User guide](user_guide.md)

`qubit-ioc` retains stop and optional wait actions only for components returned
as `Managed<T>`. Ordinary components remain the application's responsibility.
Register managed factories during assembly, but create the resource inside the
factory. Factories run only after the selected dependency graph validates, so
an unselected or invalid graph cannot start the worker early.

The complete executable example is
[`examples/app_lifecycle.rs`](../examples/app_lifecycle.rs), run with:

```bash
cargo +1.94.0 run --example app_lifecycle --no-default-features --locked
```

The example starts a Tokio task when the managed factory executes. Its stop
callback sends a one-shot signal synchronously. Its wait callback awaits the
task handle, and the program asserts that the task has exited. Tokio belongs to
the example's dev-dependencies; the runtime library does not depend on it.

`ApplicationContext::begin_shutdown(self)` consumes the context, attempts all
stop callbacks in reverse construction order, and returns a
`ShutdownHandle`. Call and await `ShutdownHandle::wait(&mut self)` to wait for
the workers in that order and collect cleanup errors. A stop error or unwind
panic is recorded while later stop callbacks still run. A panic while creating
or polling a wait future is also recorded while later waits continue.

If the caller cancels `wait`, keep the same handle and call `wait` again; the
currently pending wait future resumes. Dropping the handle abandons unfinished
waits after stop has already run. Dropping an `ApplicationContext` without
calling `begin_shutdown` does not invoke managed callbacks. A cloned component
`Arc` can keep the value alive after shutdown completes.

Sync build failure stops managed values already returned to the container but does not await
them. Async build failure stops and waits, then returns the original build
error together with cleanup failures. Cancelling an async build stops values already
returned as `Managed` without waiting; the cancelled caller cannot receive stop
errors. Effects created by a factory before it returns `Managed<T>` are owned
by that factory and must be cleaned up there.

An already-running external resource can be registered with
`register_instance(Arc<T>)`; the application keeps its shutdown responsibility.
Do not wrap that existing resource in a factory merely to attach cleanup after
it has already started.

## Construction and cleanup stages

Registration only stages definitions. Graph validation and the synchronous
build's async-factory preflight run before any factory or alias projector. A
selected synchronous managed factory can also run during an async build. Its
successful return transfers the resource and cleanup actions to the container;
a later factory may still fail and trigger rollback. Factories run serially.

| Trigger | Stop for resources already returned as `Managed` | Wait |
| --- | --- | --- |
| Graph validation or sync async-preflight fails | No resources have been constructed | None |
| Later factory fails in a sync build | Once each, reverse construction order | None |
| Later factory fails in an async build | Once each, reverse construction order | Wait in that order before returning the build error |
| Async build future is dropped | Once each, reverse construction order | None; the factory owns effects before transfer |
| Context is dropped without shutdown | None | None |
| `begin_shutdown()` | All stops run before the call returns | Await the returned handle explicitly |
| Shutdown handle is dropped | Stops have already run and are not repeated | Remaining waits are abandoned |
| Cancelled `wait()` is called again on the same handle | No repeated stop | Resume the retained active wait future |

`Managed<T>` and `ShutdownHandle` are `must_use` types. Return the managed value
from its factory so cleanup can be tracked. Dropping it before transfer does
not invoke stop or wait. Keep the shutdown handle and await `wait()` to observe
termination and errors; use `drop(context.begin_shutdown())` when intentionally
requesting stop while abandoning waits. Neither annotation introduces cleanup
in `Drop`.

Collection injection through `BuildContext::get_all` and post-build
`ApplicationContext::get_all` both use ascending `order`, ID, and source
location, preserving registration order for complete ties. This collection
order is separate from dependency-first construction and its reverse cleanup
order. Lookup uses the existing immutable type and exact-key indexes;
selection and collection sorting still happen per query.

The downstream lifecycle contract uses two separately locked dependency
snapshots: [historical](../tests/fixtures/application_consumer/Cargo.toml) and
[current](../tests/fixtures/application_consumer_current/Cargo.toml). Both CI
lanes check, test, and run the selected consumer sources with `--locked` and
record their SHAs, toolchain, and lock hash. The current lane's pin must contain
the accepted lifecycle tests; the historical snapshot only runs tests present
at its own pin. See the [current design](complete-design.md) for this boundary.
