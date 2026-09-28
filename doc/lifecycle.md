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
cargo run --example app_lifecycle --no-default-features
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

Sync build failure stops managed values already constructed but does not await
them. Async build failure stops and waits, then returns the original build
error together with cleanup failures. Cancelling an async build stops already
constructed values without waiting; the cancelled caller cannot receive stop
errors. Effects created by a factory before it returns `Managed<T>` are owned
by that factory and must be cleaned up there.

An already-running external resource can be registered with
`register_instance(Arc<T>)`; the application keeps its shutdown responsibility.
Do not wrap that existing resource in a factory merely to attach cleanup after
it has already started.
