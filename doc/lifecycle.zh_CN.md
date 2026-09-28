# 托管组件生命周期

[English lifecycle guide](lifecycle.md) · [中文用户手册](user_guide.zh_CN.md)

`qubit-ioc` 只为显式返回 `Managed<T>` 的组件保留 stop 和可选 wait 动作。普通组件由
应用负责。托管 factory 在登记时不会启动资源；它只会在所选依赖图验证通过后运行，
因此未选中的定义或无效图不会提前启动 worker。

完整的可执行示例位于[`examples/app_lifecycle.rs`](../examples/app_lifecycle.rs)，运行：

```bash
cargo run --example app_lifecycle --no-default-features
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

同步 build 失败会停止已完成构造的托管资源，但不会等待它们。异步 build 失败会 stop
并 wait，再把原 build 错误与清理错误一并返回。取消异步 build 只 stop 已构造资源，不
wait；被取消的调用方无法接收 stop 错误。factory 在返回 `Managed<T>` 之前产生的副作用，
由 factory 自己负责清理。

已启动的外部资源可以通过 `register_instance(Arc<T>)` 登记，应用仍保留关闭责任。不要
为了追加清理动作，把已经运行的外部资源伪装成 factory 创建的托管资源。
