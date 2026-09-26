# 应用组件生命周期

`rs-ioc` 负责创建共享组件和解析依赖，不会在容器释放时自动调用领域组件的关闭方法。应用应保存需要关闭的组件句柄，并根据各服务的关闭契约显式编排关闭。

```rust
let mut builder = ApplicationContext::builder().discover()?;
builder.root::<ApplicationService>();
let context = builder.build_async().await?;
let service = context.get::<ApplicationService>()?;
service.start().await?;

// 应用退出时执行；关闭 API 与等待终止是两个独立步骤。
service.shutdown().await?;
```

下游消费者夹具展示了真实的 `qubit-event-bus` 与 `qubit-execution-services` 调用：先执行 `EventBus::shutdown(ShutdownMode::Immediate)?`，再调用 `ExecutionServices::shutdown()` 并等待 `await_termination().await`。Tokio runtime 必须保持运行直到执行服务终止。`TaskExecutionService::shutdown().await` 是异步关闭并返回 `Result`。应用需要自行持有服务句柄并按依赖关系安排顺序；克隆出的 `Arc` 可能延长对象存活时间，因此释放 `Arc` 不是关闭协议。该夹具位于 `rs-execution-services/tests/fixtures/ioc_application_consumer`，是下游契约示例，不表示已有生产应用采用。

`build_async` 被取消或工厂返回错误时，容器不会撤销已经产生的外部副作用。会在工厂中启动任务或线程的组件，应由应用建立独立关闭路径，或在工厂中使用 RAII/取消守卫。容器只保证失败时不会发布部分 `ApplicationContext`。

可运行的最小示例见 `cargo run --example app_lifecycle`；跨库装配和关闭流程见上述消费者夹具。
