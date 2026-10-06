# qubit-ioc

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc.svg?color=blue)](https://crates.io/crates/qubit-ioc)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

`qubit-ioc` 帮助 Rust 应用开发者在启动时组装跨 crate 的共享组件。它先验证依赖，
再执行工厂，让缺失或歧义的服务在启动阶段暴露，而不是留到请求处理时。构建成功后
返回独占生命周期的 `Application`，通过可克隆的只读 `ApplicationContext` 获取组件；
既能用过程宏声明，也能手动注册。

## 安装

在依赖中添加 `qubit-ioc`：

```toml
[dependencies]
qubit-ioc = "0.3"
```

默认启用 `macros` 和 `config`；只使用手动注册时，在任一依赖声明中添加
`default-features = false`。

| feature | 默认 | 用途 |
| --- | --- | --- |
| `macros` | 开 | 提供 `#[Component]`、`#[Service]`、`#[Repository]`、`#[Configuration]`、`#[ConfigurationProperties]` 和 `#[bean]`。 |
| `config` | 开 | 注册 `qubit-config` 快照，并使用 `#[value]`、`#[ConfigurationProperties]`。 |

`macros` 与 `config` 可独立启用。`#[value]` 和 `#[ConfigurationProperties]`
需要同时启用这两个 feature；只启用 `config` 不会导出这些宏。

手工工厂可使用 `register_injected_factory`、
`register_injected_managed_factory`、`register_injected_async_factory` 和
`register_injected_managed_async_factory`，由类型化参数元组生成依赖请求。
元组支持 `()`、必需的 `Arc<T>`、可选的 `Option<Arc<T>>` 和全部候选
`Vec<Arc<T>>`，参数个数为零至八个。选中的异步工厂需要通过 `build_async()`
或 `build_all_async()` 构建。需要按具名 ID 或自定义依赖请求时，直接使用
`register_async_factory` 或 `register_managed_async_factory`。

下面是完整的异步注册示例；依赖请求由参数类型 `Arc<String>` 自动生成：

```rust
use std::error::Error;
use std::sync::Arc;
use qubit_ioc::ContainerBuilder;

async fn build_message_length() -> Result<(), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(String::from("hello")))?;
    builder.register_injected_async_factory::<usize, (Arc<String>,), _>(
        |(message,)| Box::pin(async move { Ok(Arc::new(message.len())) }),
    )?;
    let application = builder.build_all_async().await?;
    assert_eq!(*application.context().get::<usize>()?, 5);
    Ok(())
}
```

应在依赖图验证通过后创建托管资源。工厂返回 `Managed<T>` 之前产生的副作用，仍由工厂自行负责。

托管关闭还可使用 `WaitPolicy::bounded_with_total(grace, termination, total, timer)`
增加应用级总时限，同时保留逐组件时限。时限与报告语义见[生命周期说明](doc/lifecycle.zh_CN.md)。

## 快速开始：组装问候服务

假设应用需要跨 crate 注入问候服务。先声明具体组件及其 trait 绑定，再让服务通过
`Arc<dyn Trait>` 请求依赖，并把服务选为构建根节点。
由于 trait 的 `impl` 位于独立语法项，组件宏不会自动枚举它，因此需要写明
`bind = dyn Greeting`。

```rust
use std::sync::Arc;
use qubit_ioc::{Application, Component, Service};

trait Greeting: Send + Sync {
    fn text(&self) -> &'static str;
}

#[Component(bind = dyn Greeting, id = "example.greeting.english", primary)]
struct English;

impl Greeting for English {
    fn text(&self) -> &'static str { "hello" }
}

#[Service]
struct Greeter {
    greeting: Arc<dyn Greeting>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = Application::builder();
    builder.install::<English>()?;
    builder.install::<Greeter>()?;
    builder.root::<Greeter>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(context.get::<Greeter>()?.greeting.text(), "hello");
    Ok(())
}
```

运行 `cargo run --example readme_declarative` 可执行此示例，完整源码见
[`examples/readme_declarative.rs`](examples/readme_declarative.rs)。`build()` 至少需要一个
选定的根节点；若要构造所有已注册定义，请使用 `build_all()`。

断言能观察到选中的实现。构建器会在创建组件前验证该服务的依赖链。

### 函数 bean 与配置组

函数 bean 会生成注册 marker：`default_value` 默认生成 `DefaultValueBean`，
`#[bean(marker = CustomFactory)]` 则使用指定名称。安装这些 marker，再调用
`#[Configuration]` 模块的 `grouped::register_ioc(&mut builder)?`，选好根节点后
构建并读取组件。[函数 bean 示例](examples/readme_beans.rs) 通过断言验证 `1`、`2`
和 `"ready"`，只需启用 `macros`：

```bash
cargo +1.94.0 run --example readme_beans --no-default-features --features macros --locked
```

完整代码、异步构建和托管关闭步骤见[用户手册的函数 bean 场景](doc/user_guide.zh_CN.md#场景函数-bean-与配置组)。

## 核心能力

字段和 bean 参数可使用 `#[inject(id = "...")]` 精确选择绑定；
`Option<Arc<T>>` 表示可缺省，`Vec<Arc<T>>` 注入全部候选。`#[bean]` 可标注同步或
异步自由函数，也支持返回 `Managed<T>` 或 `Result<Managed<T>, E>` 的托管工厂；
图中含异步工厂时须调用 `build_async()`。
返回类型可使用导入的 `Managed`、`qubit_ioc::Managed` / `::qubit_ioc::Managed`。
重命名依赖和 raw keyword crate 名也受支持，例如 `ioc::Managed` / `::ioc::Managed`
以及 `r#type::Managed` / `::r#type::Managed`；这些路径也适用于
`Result<Managed<T>, E>`。`application::Managed` 等无关路径和类型别名仍按普通组件类型处理。
宏会把 `cfg` 激活条件应用到生成的依赖请求、注册代码和工厂实参，确保带条件编译的
bean 参数与生成调用保持一致。`inject`、`value` 等 helper 属性必须直接写在组件字段或 bean 参数上；
放进 `cfg_attr` 会收到明确诊断。跨定义键冲突会在 profile 过滤后的构建阶段检查，
而不是在 `register_instance_with` 暂存实例时检查。
托管资源应在图验证通过后由托管工厂创建。装配前已启动的外部资源使用
`register_instance(Arc<T>)` 注入，并由应用负责关闭；不要在工厂闭包中捕获已创建的
`Managed<T>`。

应用保留唯一生命周期所有者，通过 `application.context().clone()` 分发可并发查询的
上下文句柄；关闭时这些克隆仍可存在，但查询成功不保证组件仍接受工作。集合查询
顺序在上下文发布时按绑定 order、ID 和源码位置预先计算。`ApplicationContext::state()`
只报告生命周期状态，不会在关闭后阻止查询。正常退出调用
`application.begin_shutdown(ShutdownMode::Graceful)`，再显式等待返回的句柄。
Graceful 请求从首次轮询 `wait()` 开始；`ShutdownMode::Immediate` 会在
`begin_shutdown` 返回前请求全部 abort。
选中托管定义时必须配置 `WaitPolicy`；实际应用用 `WaitPolicy::bounded` 和由应用驱动的
计时器。自定义工厂与 trait alias 可使用公开的 `Definition::builder()` 和
`register_definition`。stop 成功即确认终止时选择 `Managed::synchronous`；需要
先发送非阻塞停止请求、再等待终止时选择 `Managed::asynchronous`。需要优雅关闭时，在构造
阶段选择 `Managed::synchronous_with_graceful` 或
`Managed::asynchronous_with_graceful`。关闭请求返回 ticket 时，使用
`Managed::asynchronous_with_ticket`；有独立 Graceful 请求时使用
`Managed::asynchronous_with_graceful_ticket`，让请求和 wait 回调共享同一个 ticket。
所选构造函数会固定 graceful 回调，创建后不能再替换。以下最小路径无需外部运行时即可编译：

```rust
use std::sync::Arc;
use qubit_ioc::{BuildError, BuildFailure, CleanupError, Managed};

async fn ticket_and_failure() {
    let _managed = Managed::asynchronous_with_graceful_ticket(
        Arc::new(()),
        |_| Ok::<u64, CleanupError>(2),
        |_| Ok::<u64, CleanupError>(1),
        |_, ticket| Box::pin(async move {
            assert!(ticket == 1 || ticket == 2);
            Ok(())
        }),
    );
    let mut failure = BuildFailure::from(BuildError::NoRootsSelected);
    assert!(failure.wait_cleanup().await.is_none());
}
```

ticket 析构不得取消资源关闭。具体适配方式见
[托管资源适配指南](doc/managed-adapters.zh_CN.md)，完整关闭与 0.3 迁移步骤见
[生命周期指南](doc/lifecycle.zh_CN.md)。

`#[value]` 与 `ConfigurationProperties` 读取保存的原始值，不会自动插值。
结构化反序列化默认拒绝未知字段。需要插值时，可在工厂中显式调用
`Config::get_interpolated`。
工厂 panic 遵循 Rust 的 panic 语义并向外传播。同步和异步构建失败时，都会先为已移交
资源请求 abort，再立即返回 `BuildFailure`。调用 `failure.wait_cleanup().await` 可观察
可选的 `ShutdownReport`，同时通过 `cause()` 保留原始 `BuildError`。若等待被取消，可在
同一个 failure 上再次调用以继续观察清理；清理错误保留在报告中，不会替换构建错误。
需要自行管理清理句柄时，仍可使用底层 `take_cleanup()` 或 `into_parts()`。

### 手动组装

关闭默认 feature 后，仍可直接注册实例和工厂：

```rust
use std::sync::Arc;
use qubit_ioc::{ContainerBuilder, Dependency};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(String::from("hello")))?;
    builder.register_factory::<usize, _>(&[Dependency::of::<String>()], |context| {
        let message = context.get::<String>().expect("declared dependency");
        Ok(Arc::new(message.len()))
    })?;
    builder.root::<usize>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(*context.get::<usize>()?, 5);
    Ok(())
}
```

运行 `cargo run --example readme_manual --no-default-features` 可执行此示例，完整源码见
[`examples/readme_manual.rs`](examples/readme_manual.rs)。

绑定身份由 Rust 类型和可选 ID 共同决定。ID 使用 `example.greeting.english` 这样
以点分隔的 ASCII 段；每段以字母开头，后续可包含字母、数字或下划线。未指定 ID 的
请求会选择唯一候选，或多个候选中唯一标记为 `primary` 的绑定。

应用通过显式清单组装定义。构建根节点、profile、候选选择、错误处理和
关闭责任详见[用户手册](doc/user_guide.zh_CN.md)。下游应用装配示例见
[`rs-execution-services` 消费者夹具](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/README.md)。
在 `rs-execution-services` 仓库根目录运行
`cargo run --manifest-path tests/fixtures/ioc_application_consumer/Cargo.toml`。
该夹具用于验证跨 crate 契约，不代表已有生产应用采用。
夹具的 EventBus 适配器用 `request_shutdown` 请求关闭，再等待 ticket 的
`wait_async()`；同步 EventBus `shutdown` 即使选择 Immediate 模式也会等待。

## 限制

当前只提供应用级共享实例，不提供原型或请求作用域、热更新、未托管组件的自动关闭、
循环代理和动态库发现。托管工厂可通过 `Managed<T>` 和
`Application::begin_shutdown` 与 `ShutdownHandle::wait` 执行关闭动作。结构体宏支持具名字段和单元结构体，其他形状可使用手动工厂。
组件构造不依赖运行时反射。`qubit-spi` 继续负责 provider 的选择和回退；其注册表或
选中的服务可作为普通 IoC 组件注册。要观察托管资源关闭结果，需显式指定关闭模式并等待 `ShutdownHandle::wait()`。
丢弃 owner、未移交的 `Managed` 或关闭句柄会尽力请求 abort，不会等待；丢弃查询
上下文不会请求关闭。期限不能杀死阻塞工作，`ShutdownReport::incomplete()` 表示尚未确认终止。完整流程见[生命周期指南](doc/lifecycle.zh_CN.md)与可运行的
[`app_lifecycle`示例](examples/app_lifecycle.rs)，设计边界见
[English current design](doc/complete-design.md)和[中文当前设计](doc/complete-design.zh_CN.md)。

## 延伸阅读

按[中文用户手册](doc/user_guide.zh_CN.md)或[English user guide](doc/user_guide.md)
完成安装、构建、诊断和关闭流程。[托管资源适配指南](doc/managed-adapters.zh_CN.md)
及[English managed adapter guide](doc/managed-adapters.md)说明停止与等待契约。
在源码目录运行 `cargo doc --no-deps --open`
可查看公开 API 文档。

## 测试

```bash
# 使用默认 feature 集运行测试
cargo test

# 使用项目声明的全部 feature 运行测试
cargo test --all-features

# 运行项目 CI 检查
.infra/bin/ci-check.sh

# 检查代码覆盖率
.infra/bin/coverage.sh
```

## 许可证

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

本项目基于 Apache License 2.0 授权。完整许可证文本请参阅
[LICENSE](LICENSE)。

## 贡献

欢迎贡献。请遵循 Rust API 指南，及时更新公共 API 文档与测试，并在提交
Pull Request 前运行 `.infra/bin/align-ci.sh` 格式化代码，运行 `.infra/bin/ci-check.sh` 对齐 CI 要求。

## 作者

**Haixing Hu** - *Qubit Co. Ltd.*

仓库地址：[https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)
