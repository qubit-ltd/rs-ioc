# 托管组件生命周期

[English lifecycle guide](lifecycle.md) · [中文用户手册](user_guide.zh_CN.md)

`qubit-ioc` 只为显式返回 `Managed<T>` 的组件保留 stop 和可选 wait 动作。普通组件由
应用负责。托管 factory 在登记时不会启动资源；它只会在所选依赖图验证通过后运行，
因此未选中的定义或无效图不会提前启动 worker。

完整的可执行示例位于[`examples/app_lifecycle.rs`](../examples/app_lifecycle.rs)，运行：

```bash
cargo +1.94.0 run --example app_lifecycle --no-default-features --locked
```

示例在托管 factory 执行时启动 Tokio task。同步 stop 回调发送 one-shot 停止信号；
wait 回调 await task handle；主函数检查 task 已退出。Tokio 只是示例的 dev-dependency，
运行时库本身不依赖 Tokio。

`ApplicationContext::begin_shutdown(self)` 消费 context，按逆构建顺序尝试所有 stop，
并返回 `ShutdownHandle`。调用并 await `ShutdownHandle::wait(&mut self)`，按相同顺序
等待 worker 结束并汇总错误。stop 错误或 unwind panic 会记录下来，但不会阻止后续
stop；创建或轮询 wait future 时发生的 panic 也会记录，并继续执行后续 wait。

如果调用方取消 `wait`，保留同一个 handle 后再次调用 `wait`，会从当前 pending future
继续。丢弃 handle 会放弃未完成的 wait；stop 已经执行。未调用
`begin_shutdown` 就丢弃 `ApplicationContext` 不会触发托管回调。外部持有的组件 `Arc`
可在 shutdown 完成后继续保持值存活。

同步 build 失败会停止已成功返回 `Managed` 的资源，但不会等待它们。异步 build 失败会 stop
并 wait，再把原 build 错误与清理错误一并返回。取消异步 build 只 stop 已成功返回 `Managed` 的资源，不
wait；被取消的调用方无法接收 stop 错误。factory 在返回 `Managed<T>` 之前产生的副作用，
由 factory 自己负责清理。

已启动的外部资源可以通过 `register_instance(Arc<T>)` 登记，应用仍保留关闭责任。不要
为了追加清理动作，把已经运行的外部资源伪装成 factory 创建的托管资源。

## 构建与清理阶段

注册只登记定义；图验证和同步构建的异步工厂预检都在运行任何工厂或 alias projector
之前完成。选中的同步托管工厂也可以在异步构建中运行。工厂成功返回后，资源及清理
动作才移交给容器；后续工厂仍可能失败并触发回滚。工厂始终串行执行。

| 触发条件 | 对已成功返回 `Managed` 的资源执行 stop | wait |
| --- | --- | --- |
| 图验证或同步异步工厂预检失败 | 尚未构造资源 | 不执行 |
| 同步构建中后续工厂失败 | 按逆构建顺序各执行一次 | 不执行 |
| 异步构建中后续工厂失败 | 按逆构建顺序各执行一次 | 按该顺序等待后才返回构建错误 |
| 丢弃异步构建 future | 按逆构建顺序各执行一次 | 不执行；移交前的副作用由工厂负责 |
| 未关闭就丢弃 context | 不执行 | 不执行 |
| `begin_shutdown()` | 返回前执行全部 stop | 显式等待返回句柄 |
| 丢弃关闭句柄 | stop 已执行，不会重复 | 放弃剩余等待 |
| 取消 wait 后在同一句柄上再次调用 `wait()` | 不重复 stop | 继续保留的活跃 wait future |

`Managed<T>` 和 `ShutdownHandle` 都带有 `must_use` 提示。工厂应返回托管值，让
容器跟踪清理；移交前直接丢弃该值不会执行 stop 或 wait。保留关闭句柄并等待
`wait()`，才能观察终止与清理错误；若有意只请求停止而不等待，请显式使用
`drop(context.begin_shutdown())`。这两个提示都不会在 `Drop` 中增加清理动作。

`BuildContext::get_all` 的集合注入和构建后 `ApplicationContext::get_all` 的结果，
都按 `order`、ID、来源位置升序排列，完全相同的项保留注册顺序。集合顺序与依赖
优先的构建顺序、逆构建顺序的清理各有用途。查询沿用不可变的类型与精确键索引；
候选选择和集合排序仍在每次查询时完成。

下游生命周期契约使用独立锁定的两套依赖快照：[历史基线](../tests/fixtures/application_consumer/Cargo.toml)
和[当前快照](../tests/fixtures/application_consumer_current/Cargo.toml)。两条 CI lane
都用 `--locked` 对选定源码执行 check、test、run，并记录 SHA、工具链和 lock 哈希。
当前 lane 的固定提交必须包含已接受的生命周期测试；历史 lane 只运行自身提交已有的
测试。边界说明见[当前设计](complete-design.zh_CN.md)。
