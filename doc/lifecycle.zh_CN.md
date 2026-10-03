# 托管组件生命周期

## 应用级关闭时限

`WaitPolicy::bounded(grace, termination, timer)` 保留逐组件时限。需要应用级预算时，
使用 `bounded_with_total(grace, termination, total, timer)`。总时限在
`ShutdownHandle::wait()` 首次被 poll 时启动；借用的 wait future 被取消或关闭模式升级后
仍复用同一计时器，调用 `abort()` 也不会重置。空应用、`abandon()` 和 Drop 不启动总计时器。

总计时器到期后，IoC 会请求所有未确认托管组件 abort，并在
`ShutdownReport::overall_failure()` 记录原因。仍未确认的组件列入 `incomplete()`；
此前的失败和回退信息会保留。`is_complete()` 表示已确认终止，`is_success()` 还要求没有
全局失败。时限无法中断同步回调、一次阻塞的 future poll 或析构函数。

## 类型化同步工厂

`register_injected_factory` 和 `register_injected_managed_factory` 接收显式参数元组。
`Arc<T>` 声明必需请求，`Option<Arc<T>>` 声明可选请求，`Vec<Arc<T>>` 声明集合请求。
元组支持零至八个参数；完全相同的请求在依赖图中去重，但每个参数都会分别解析并传给
工厂。需要 ID、异步工厂或更多参数时继续使用现有注册方法。

[English lifecycle guide](lifecycle.md) · [中文用户手册](user_guide.zh_CN.md) · [托管资源适配指南](managed-adapters.zh_CN.md)

本文适用于 `qubit-ioc` 0.3.0。装配阶段只登记托管工厂，资源在所选依赖图验证通过后
才由工厂创建；成功返回 `Managed<T>` 后，资源及清理动作才移交给容器。已经启动的
外部资源可以通过 `register_instance(Arc<T>)` 注入，关闭责任仍由应用承担。
工厂返回 `Managed` 前产生的副作用，包括未完成构造被取消时的资源，由工厂自身的
RAII guard 收尾。

[可运行的 worker 示例](../examples/app_lifecycle.rs) 在工厂里启动 Tokio task，发送
one-shot 停止信号，等待 task handle 并检查任务确实退出：

```bash
cargo +1.94.0 run --example app_lifecycle --no-default-features --locked
```

Tokio 是示例依赖，IoC 核心不依赖它。没有独立排空协议的 worker 可以在 Graceful
流程中降级为 abort，报告会把这个组件列入 `ShutdownReport::fallbacks()`。

## 保留所有者，处理每条退出路径

`build`、`build_all` 及其异步版本返回 `Application`。应用保留这个唯一所有者，
通过 `application.context().clone()` 分发只读查询句柄；关闭期间这些克隆仍可存在。
`context.state()` 可观察 Running、ShuttingDown、Closed 或 Incomplete。关闭开始后
查询仍能返回保存的 `Arc`，但不代表服务还接收业务工作；接收能力由组件自身控制。
外部克隆也可能在资源终止后继续保持对象内存存活。

选中托管定义时必须显式配置 `WaitPolicy`，否则工厂执行前会收到 cause 为
`BuildError::MissingWaitPolicy` 的 `BuildFailure`。真实应用使用
`WaitPolicy::bounded`。下面是集成函数：调用方先登记资源，再传入 builder 和应用
业务操作；函数在已启用 time 的 Tokio runtime 中运行（`tokio` 的 `rt`、`time`
feature）。它完整处理启动失败和业务失败，正常退出选择 Graceful：

```rust
use std::error::Error;
use std::time::Duration;
use qubit_ioc::{ApplicationContext, ContainerBuilder, ShutdownMode, WaitPolicy};

// 调用方先登记应用资源，再调用本函数。
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
        Err(failure) => {
            let settled = failure.settle().await;
            if let Some(report) = settled.cleanup_report() {
                if !report.is_success() {
                    eprintln!("rollback report: {report:?}");
                }
            }
            let (cause, _) = settled.into_parts();
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

业务操作先把结果交回仍然持有 owner 的应用，再决定关闭方式。异步业务也采用同一
结构：保留 owner，把业务 await 结果保存下来，选择模式，等待关闭后再返回结果。
若业务和清理同时失败，此片段记录清理报告并返回业务错误；应用也可定义同时保留
两者的错误类型。

同步和异步构建在后续工厂返回错误时，都先为已经移交的托管资源请求 abort，再立即
返回 `BuildFailure`。`settle().await` 只等待一次可选回滚，返回同时保存原始
`BuildError` 和可选 `ShutdownReport` 的 `SettledBuildFailure`。其 `Error::source()`
指向原始构建错误及其工厂来源链。清理失败时也保留报告，可检查 `is_success()`、
`failures()` 和 `incomplete()`。图验证或预检失败时没有已创建资源，
`cleanup_report()` 为 `None`。`take_cleanup()` 和 `into_parts()` 仍可用于底层
清理所有权转移。异步构建内部和 `BuildFailure` Drop 都不等待回滚。若取消 `settle()`
future，丢弃清理句柄只请求剩余 abort，无法取得最终报告。工厂的 unwind panic 仍
向外传播；构建取消或栈展开只请求 abort，不执行 wait。

## 请求契约与关闭顺序

创建托管值时就应选定终止确认方式。`Managed::synchronous(value, stop)` 适用于
stop 回调成功返回就表示资源已终止的情况。后台资源使用
`Managed::asynchronous(value, abort, wait)`：`abort` 是同步、非阻塞的取消请求，
`wait` 用来确认终止。请求返回 ticket 时使用 `Managed::asynchronous_with_ticket` 或
`Managed::asynchronous_with_graceful_ticket`；未使用的 ticket 被丢弃时不能取消资源
关闭。abort 不能 join、block_on、等待条件变量、执行业务 handler
或进行无界 I/O。`.with_graceful_stop(request)` 只请求当前组件停止接收工作并排空，
不能顺手关闭依赖。不能用 ready future 冒充尚未退出的后台任务。两种构造方式和
ticket 等待见[托管资源适配指南](managed-adapters.zh_CN.md)。

- `application.begin_shutdown(ShutdownMode::Graceful)` 转移所有权并发布 ShuttingDown，
  首次轮询 `wait()` 才开始请求。按逆构建顺序逐个请求消费者排空，等待它终止后才
  请求关闭其依赖；缺少 graceful 回调的组件会降级为 abort，记入 `fallbacks()`。
- `application.begin_shutdown(ShutdownMode::Immediate)` 返回前按逆构建顺序请求所有
  abort，随后由显式 `wait()` 按同一顺序等待。某个请求报错或 unwind panic 不会阻止
  后续请求。失败和取消采用这条 abort 路径。

只有成功构建的具体托管定义产生生命周期条目，trait alias 不增加重复条目。
注册和图验证阶段不会运行工厂或 alias projector；构建和关闭均串行执行。

## 期限、取消恢复与报告

bounded policy 为每个组件分别提供 grace 和 termination 预算。Graceful wait 超时后
记录错误，给当前组件请求 abort，保留同一个 pending wait 并切换到 termination
预算；再次超时则标记 incomplete，继续后续组件。graceful 请求失败也会退回 abort；
wait 报错或 panic 后，已消费的一次性 wait 不能重新创建来证明终止。终止预算耗尽时，
尚未结束的消费者不能再获得“先结束消费者、再关闭依赖”的完整保证。

应用必须正常驱动计时器。期限只约束会返回 Pending 的 future，无法打断阻塞回调、
阻塞的 `Future::poll`、析构函数或 `panic = "abort"`。创建或轮询计时器时的 unwind
panic 记录为 Deadline 失败，并继续后续组件。`WaitPolicy::unbounded()` 明确允许无限
等待，适用于受控测试，但没有有限终止保证。

取消借用句柄的 `wait()` future 不会丢弃当前 wait、deadline、阶段与预算；在同一
句柄上再次调用会接着执行，不会重新创建动作或重置期限。`abort()` 可将所有未完成
条目升级为 Immediate，不重复 abort；保留当前 pending wait，也不会重置已开始的
termination 预算。`pending()` 列出尚未确认终止的组件。

`wait()` 成功返回 `ShutdownReport`；失败返回携带该报告的 `ShutdownError`。
`failures()` 保留绑定键、来源、阶段和原始清理错误。`is_complete()` 只检查
`incomplete()` 是否为空；`is_success()` 还要求没有 failures。即使全部终止得到确认，
仍可能有回调错误；单纯 graceful fallback 不算失败。重复等待已完成的句柄会返回
相同观察结果，不会重复动作；即使通过 `abort()` 升级，`mode()` 仍保留最初请求的模式。

Incomplete 表示尚未确认终止，不代表运行时杀死了资源。资源可能继续运行并持有
外部句柄；应用应检查报告并按资源自身的恢复协议处理，不能把超时当成工作已经消失。

## Drop 与主动放弃等待

`Application`、未移交的 `Managed` 和 `ShutdownHandle` Drop 会尽力为所拥有的未完成
条目请求 abort，不创建或轮询 wait。Drop 无法返回清理错误，也不能证明清理成功。
查询 context Drop 不请求关闭。显式调用 `shutdown.abandon()` 会请求剩余 abort，
返回把未确认终止条目标成 incomplete 的报告，不启动新 wait。Graceful 句柄首次
poll 前就被丢弃时，会请求 abort；仅创建句柄不表示已经完成排空请求。owner 和关闭
句柄不可克隆，`must_use` 提醒应用观察生命周期，不承诺 Drop 会完成清理。

## 接入 EventBus

`qubit-event-bus` 0.20.0 的同步 `EventBus::shutdown(Immediate)` 仍会等待 worker 和
provider，因此不能放进托管 abort 或 graceful 请求回调。请求回调应调用
`EventBus::request_shutdown(mode)`，由 `Managed::asynchronous_with_graceful_ticket`
管理返回的 ticket，在托管 wait 回调里等待 `ticket.wait_async()`。Graceful 请求停止接收工作并排空；随后
Immediate 请求会加强同一次关闭。ticket 绑定那次关闭的 generation，丢弃 ticket
或取消异步观察都不会取消后台关闭。`ticket.wait(timeout)` 限制同步观察者的等待，
IoC `WaitPolicy` 限制异步 wait；两者都不能强制杀死 provider。用 `spawn_blocking`
包装同步 shutdown 也不符合 abort 请求契约。
[当前下游消费夹具说明](../tests/fixtures/application_consumer_current/SOURCE.md)
记录了 request/ticket 适配器的验证范围，以及构建失败和业务退出时的清理责任。

## 从 0.2 迁移到 0.3

这是破坏性变更；下表左列仅作为历史迁移输入。运行时和宏都须使用 0.3.0，旧生命周期
和旧代码生成协议不提供兼容层。

| 原调用或行为 | 0.3 替代方式及责任 |
| --- | --- |
| `ApplicationContext::builder()` | 使用 `Application::builder()` 或 `ContainerBuilder::new()`。 |
| build 返回 `ApplicationContext` | 返回 `Application`；通过 `application.context()` 查询，需要共享时克隆查询句柄。 |
| 关闭前 `Arc::try_unwrap(context)` | 保留唯一 Application owner，直接由它关闭；查询句柄克隆可继续存在。 |
| `begin_shutdown()` 固定先停止全部 | 正常退出选择 `begin_shutdown(ShutdownMode::Graceful)`，失败选择 `Immediate`；Graceful 首次 poll wait 才开始请求。 |
| 异步 build 等待回滚后才报错 | `BuildFailure` 在请求 abort 后立即返回；应用等待 `settle()` 取得原构建错误与可选清理报告。`take_cleanup()` / `into_parts()` 仍可供底层管理所有权。`BuildError::CleanupFailed` 已移除，清理错误进入关闭报告。 |
| context / `Managed` Drop 不清理 | 查询 context 仍无关闭责任；owner、未移交 `Managed` 和关闭句柄 Drop 会请求 abort，不 wait。 |
| 隐藏 `codegen_v1::DefinitionDraft` | 使用公开 `Definition::builder()` 与 `register_definition`；宏复用同一核心，仅配置诊断和生成代码 glue 仍隐藏。 |
| 以 `EventBus::shutdown` 作为 stop | 通过 ticket 适配器调用非阻塞的 `request_shutdown`，在 wait 回调中等待 `wait_async()`。 |
| 集合每次查询时排序 | context 发布时按 order、ID、来源和注册位置预排序不可变类型索引，查询复用其顺序。 |

集合注入和构建后查询仍按 order、ID、来源位置升序排列，完全相同的项保留注册顺序；
它与构建、清理顺序各有用途。精确键与类型索引消除了特定候选和成员的重复扫描，
但不承诺整体图算法严格线性，也不承诺固定耗时或生产环境加速倍数。

下游验证保留独立锁定的[历史快照](../tests/fixtures/application_consumer/Cargo.toml)和
[当前快照](../tests/fixtures/application_consumer_current/Cargo.toml)。两份 manifest
和 lockfile 现均指定 EventBus 0.20；历史 lane 保留较早的消费者源码，不覆盖当前的
request/ticket 适配器。各 lane 的外部 checkout 修订由工作流定义，IoC 则使用待验证的
当前修订。边界见[当前设计](complete-design.zh_CN.md)。
