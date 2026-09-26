# 应用组件生命周期

`rs-ioc` 负责创建共享组件和解析依赖。生命周期管理是显式 opt-in：托管定义在构建成功后由上下文持有关闭动作，普通定义仍由应用自行关闭。

```rust
let mut builder = ApplicationContext::builder().discover()?;
builder.root::<ApplicationService>();
let context = builder.build_async().await?;
let service = context.get::<ApplicationService>()?;
service.start().await?;

// 应用退出时执行；关闭 API 与等待终止是两个独立步骤。
service.shutdown().await?;
```

托管组件先同步请求 stop，再按逆构建顺序异步 wait；所有 stop 先完成后才开始 wait。错误会按执行顺序聚合，异步构建失败保留原始构建错误与清理错误。Tokio runtime 必须保持运行直到执行服务终止。`shutdown_async(self)` 消费上下文；外部 `Arc` 克隆仍可能延长值的存活时间。下游消费者夹具位于 `rs-execution-services/tests/fixtures/ioc_application_consumer`，展示 EventBus 与 ExecutionServices 的托管关闭调用，不表示已有生产应用采用。

异步构建被取消时，已成功构造的托管组件会收到 stop 请求，但不会等待；stop 错误无法交还给已取消的调用方。工厂在返回 `Managed<T>` 前产生的副作用仍由工厂负责清理。普通上下文 drop 不自动停止托管资源，应用需显式调用 `shutdown_async`。

可运行的最小示例见 `cargo run --example app_lifecycle`；跨库装配和关闭流程见上述消费者夹具。
