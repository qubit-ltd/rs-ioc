# 应用组件生命周期

`qubit-ioc` 只对显式标记为 `Managed<T>` 的组件保存 stop 和可选 wait 动作。
普通组件仍由应用自行管理。托管组件在成功构建后由 `ApplicationContext` 持有关闭动作；
应用应在退出时消费上下文并调用 `begin_shutdown()`，再等待返回的句柄。

```rust
use std::sync::Arc;
use qubit_ioc::{ApplicationContext, CleanupError, ContainerBuilder, Managed};

struct Worker;
impl Worker {
    fn request_stop(&self) -> Result<(), std::io::Error> { Ok(()) }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_managed_factory::<Worker, _>(&[], |_| {
        let worker = Arc::new(Worker);
        Ok(Managed::new(Arc::clone(&worker), |worker| {
            worker.request_stop().map_err(CleanupError::new)
        }))
    })?;
    builder.root::<Worker>();
    let context: ApplicationContext = builder.build_async().await?;
    let _worker = context.get::<Worker>()?;

    let mut shutdown = context.begin_shutdown();
    shutdown.wait().await?;
    Ok(())
}
```

`Managed::new` 提供同步 stop；`.with_wait` 可添加异步终止等待。正常关闭会先按逆构建顺序
调用全部 stop，再按同一顺序执行 wait，并聚合错误。`build()` 的工厂失败会 stop 已创建
资源，但不会等待；若构建失败前也必须等待资源终止，应使用 `build_async()`，即使工厂
本身都是同步的。异步构建失败会 stop 后 wait，并保留原始构建错误及清理错误。

异步构建 future 被取消时，已构造资源会收到 stop，但不会 wait；stop 错误无法返回给已
取消的调用方。工厂在返回 `Managed<T>` 前产生的副作用由工厂自己清理。普通上下文 drop
不会自动停止资源；外部 `Arc` 克隆也可能在关闭后继续持有对象。若 wait future 被取消，保留句柄并再次调用 `wait()` 可从原 future 继续等待。

下游消费者夹具位于 `rs-execution-services/tests/fixtures/ioc_application_consumer`，展示
`EventBus` 和 `ExecutionServices` 的托管关闭调用，不表示已有生产应用采用。最小示例见
`cargo run --example app_lifecycle`。
