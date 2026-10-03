# 托管资源适配指南

[English guide](managed-adapters.md) · [生命周期说明](lifecycle.zh_CN.md) · [用户手册](user_guide.zh_CN.md)

本文适用于 `qubit-ioc` 0.3.0。应用创建资源，且需要在关闭依赖前确认资源终止时，
应使用托管工厂。装配时登记工厂，构建选中依赖图时才创建资源。应用保留返回的
`Application` 作为生命周期所有者，配置由应用持续驱动且有期限的 `WaitPolicy`，
等待关闭句柄完成后再释放驱动资源的运行时。

## 根据终止方式选择构造函数

如果停止方法返回前已经让资源终止，使用 `Managed::synchronous(value, stop)`。
`stop` 成功返回就是终止确认；仅发送停止信号的方法不符合这个条件。该回调会同步
执行，包括回滚和 Drop 时，因此执行时间必须有界。

```rust
use std::sync::Arc;
use qubit_ioc::Managed;

// Worker::stop_and_join 由应用实现：返回前 worker 已退出，并提供自身的错误类型。
let managed = Managed::synchronous(Arc::clone(&worker), |worker| {
    worker.stop_and_join().map_err(qubit_ioc::CleanupError::new)
});
```

后台 worker 需要先发停止请求、再异步确认退出时，在创建时就使用
`Managed::asynchronous(value, abort, wait)`。`abort` 是同步、非阻塞的请求；
`wait` 返回拥有等待状态的 future，完成时才确认终止。worker 尚未退出时，不能用
立即就绪的 future 冒充终止。显式关闭会等待消费者终止，再关闭它的依赖；构建取消或
Drop 只请求 abort，不等待。构建失败可调用 `BuildFailure::settle().await` 等待可选的
清理句柄，同时保留原始构建错误和报告。

```rust
use std::sync::Arc;
use qubit_ioc::Managed;

// Worker::request_stop 只发送信号；Worker::join 异步确认退出。
// Worker 及这两个方法均由应用实现。
let managed = Managed::asynchronous(
    Arc::clone(&worker),
    |worker| worker.request_stop().map_err(qubit_ioc::CleanupError::new),
    |worker| Box::pin(async move {
        worker.join().await.map_err(qubit_ioc::CleanupError::new)
    }),
);
```

上面是集成片段，需由应用提供 `Worker`；可运行的 worker 见
[`app_lifecycle`](../examples/app_lifecycle.rs)。异步资源如果缺少终止等待，IoC 可能
在它仍运行时报告关闭成功，随后关闭它仍在使用的依赖。
`ShutdownReport::incomplete()` 只表示终止尚未得到确认，不表示 worker 已被杀死。

## 消费关闭 ticket

停止请求返回 ticket 时，使用
`Managed::asynchronous_with_ticket(value, abort, wait)`；Graceful 和 Immediate
分别返回 ticket 时，使用
`Managed::asynchronous_with_graceful_ticket(value, abort, graceful, wait)`。
适配器保存尚未观察的 ticket，并将其移入等待 future；内部持锁期间不执行请求回调，
也不轮询等待 future。对 EventBus 0.20，完整适配器如下：

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

同步的 `EventBus::shutdown(Immediate)` 会等待 worker 和 provider，不能用作非阻塞
abort 回调。EventBus ticket 绑定关闭 generation，丢弃 ticket 不会取消后台关闭。
**这也是上述两个 ticket 构造器的使用前提：**升级时可能丢弃未使用的 Graceful 或
Immediate ticket。若某资源丢弃 ticket 就会取消关闭，应使用底层
`Managed::asynchronous`，依资源自身契约管理观察。
[EventBus 集成夹具](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/src/managed_event_bus.rs)
验证跨 crate 契约，不代表已有生产应用采用。

`ShutdownHandle::wait()` 借用句柄；取消该借用 future 后，句柄仍保留正在进行的等待和
期限，再次调用 `wait()` 会继续观察同一 ticket。丢弃句柄会请求剩余资源 abort，
不能证明它们已经终止。

## Graceful 排空与 Immediate 升级

对底层构造器，组件支持先停止接收新工作、再排空已有工作时，可添加
`.with_graceful_stop(request)`。带 `graceful` 参数的 ticket 构造器已安装该请求。
请求只处理组件自身的接纳状态，不能顺手关闭依赖。Graceful 请求在首次轮询
`ShutdownHandle::wait()` 时开始；IoC 等待一个消费者终止后再处理其依赖。
两种 ticket 构造器均不可再链式调用 `.with_graceful_stop()`：它会添加或替换一个无法
产出 wait 所需 ticket 的 graceful 回调。Graceful ticket 请求应使用
`asynchronous_with_graceful_ticket`。
请求失败或 grace 期限到期后，IoC 会请求 abort。调用
`ShutdownHandle::abort()` 可把未完成条目升级为 Immediate，保留当前等待及其期限。

EventBus 的 graceful 回调调用 `request_shutdown(Graceful { timeout })`；升级时调用
`request_shutdown(Immediate)`，加强同一次关闭 generation。若 wait 已开始，适配器
保留正在观察的 future，丢弃新 ticket；若尚未开始，新 ticket 替换待观察的 graceful
ticket。缺少 graceful 支持的组件会退回 abort，并
记录在 `ShutdownReport::fallbacks()` 中。

[ExecutionServices 集成夹具](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/src/managed_execution_services.rs)
把 `stop()` 用作立即取消、`shutdown()` 用作 graceful 请求、
`await_termination()` 用作终止确认。它也是集成夹具，不是生产采用证据。调用方
必须在终止等待完成前保持 Tokio 运行时存活；提前释放运行时可能使等待无法完成。

## 期限与排障

`WaitPolicy` 的期限只约束会让出执行权的等待 future，且应用必须持续驱动计时器。
它无法打断阻塞的 stop 回调、阻塞的 `Future::poll`、析构函数或
`panic = "abort"`。abort 和 graceful 回调应避免阻塞；后台 worker 应在回调里
发送信号，在 `wait` 中确认退出。检查 `ShutdownReport` 中的失败阶段、graceful
降级和未确认条目。超时后应使用资源自身的恢复机制，不能假定资源已经退出。
