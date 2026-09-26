# 应用组件生命周期

`rs-ioc` 负责创建共享组件和解析依赖，不会在容器释放时自动调用领域组件的关闭方法。应用应保存需要关闭的组件句柄，并在启动成功后按依赖的逆序显式关闭。

```rust
let mut builder = ApplicationContext::builder().discover()?;
builder.root::<ApplicationService>();
let context = builder.build_async().await?;
let service = context.get::<ApplicationService>()?;
service.start().await?;

// 应用退出时执行；按启动依赖的逆序关闭。
service.shutdown().await?;
```

常见下游关闭 API 包括 `TaskExecutionService::shutdown().await`、`EventBus::shutdown(mode)` 和 `ExecutionServices::shutdown()`。应用需要自行持有相应服务并编排关闭顺序。克隆出的 `Arc` 可能延长对象存活时间，因此 `Arc` 释放不是关闭协议。

`build_async` 被取消或工厂返回错误时，容器不会撤销已经产生的外部副作用。会在工厂中启动任务或线程的组件，应由应用建立独立关闭路径，或在工厂中使用 RAII/取消守卫。容器只保证失败时不会发布部分 `ApplicationContext`。

可运行的最小示例见 `cargo run --example app_lifecycle`。
