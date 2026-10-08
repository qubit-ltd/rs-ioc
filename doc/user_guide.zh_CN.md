# qubit-ioc 用户手册

[English user guide](user_guide.md) · [项目 README](../README.zh_CN.md)

本手册面向使用 `qubit-ioc` 0.3.0 的 Rust 应用开发者，介绍如何在启动时组装
应用级共享组件、定位构建问题，以及安排资源关闭。项目要求 Rust 1.94 或更新版本。
建议先完成下面的共享设置场景，再按需要阅读 bean、构建范围、失败清理和生命周期专题。

## 概念模型

| 术语 | 含义 |
| --- | --- |
| 定义 | 已登记的实例或工厂；登记时不会执行工厂。 |
| 绑定 | 由 Rust 类型和可选的区分大小写 ID 标识的组件。 |
| 依赖 | 工厂显式声明或宏根据字段生成的请求。 |
| 根节点 | `build()` 要构建的组件及其传递依赖的起点。 |
| Application | 构建成功后返回的唯一生命周期所有者。 |
| 上下文 | 从 `application.context()` 借用或克隆的只读查询句柄。 |

构建器先筛选生效的 profile，再解析根节点和依赖、验证依赖图，最后按依赖顺序执行
工厂。`build_all()` 则构建所有生效的定义。构建失败时不会发布部分上下文。

## 场景：用共享设置启动服务

一个应用有问候文本，服务启动时需要读取它。目标是在启动阶段构建服务，看到输出，
并确认多次查询取得同一个实例。下面使用手动注册；即使关闭全部默认 feature，
这条路径也可用。

### 安装与最小配置

在应用依赖中添加 `qubit-ioc`：

```toml
[dependencies]
qubit-ioc = { version = "0.3", default-features = false }
```

将下面的代码放到该应用的 `src/main.rs`，然后在应用目录运行 `cargo run`：

```rust
use std::sync::Arc;
use qubit_ioc::{ContainerBuilder, Dependency};

struct Greeter {
    message: Arc<String>,
}

impl Greeter {
    fn greet(&self) -> &str {
        self.message.as_str()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(String::from("hello")))?;
    builder.register_factory::<Greeter, _>(&[Dependency::of::<String>()], |context| {
        let message = context.get::<String>().expect("declared dependency");
        Ok(Arc::new(Greeter { message }))
    })?;
    builder.root::<Greeter>();

    let application = builder.build()?;
    let context = application.context();
    let first = context.get::<Greeter>()?;
    let second = context.get::<Greeter>()?;
    assert_eq!(first.greet(), "hello");
    assert!(Arc::ptr_eq(&first, &second));
    println!("{}", first.greet());
    Ok(())
}
```

命令会打印 `hello`。`Dependency::of::<String>()` 声明工厂允许读取的依赖；
`root::<Greeter>()` 选择服务及其依赖一起构建。两次查询得到的是同一个已构建
组件的 `Arc` 句柄。仓库还提供启动与关闭示例，可在仓库目录运行
`cargo run --example app_lifecycle`。

## 场景：函数 bean 与配置组

provider 通过自由函数创建值时，可以用 `#[bean]` 为函数生成可安装的定义。
下面的入门练习展示默认 marker、自定义 marker，以及按模块组织的配置组。
它使用 Rust 2024、Rust 1.94，只需启用 `macros`：

```toml
qubit-ioc = { version = "0.3", default-features = false, features = ["macros"] }
```

```rust
use std::sync::Arc;

use qubit_ioc::ContainerBuilder;
use qubit_ioc::bean;

struct DefaultValue(u8);
struct CustomValue(u8);

#[bean]
fn default_value() -> DefaultValue {
    DefaultValue(1)
}

#[bean(marker = CustomFactory)]
fn custom_value() -> Arc<CustomValue> {
    Arc::new(CustomValue(2))
}

#[qubit_ioc::Configuration]
mod grouped {
    #[qubit_ioc::bean]
    fn label() -> String {
        "ready".to_owned()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.install::<DefaultValueBean>()?;
    builder.install::<CustomFactory>()?;
    grouped::register_ioc(&mut builder)?;
    builder.root::<DefaultValue>();
    builder.root::<CustomValue>();
    builder.root::<String>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(context.get::<DefaultValue>()?.0, 1);
    assert_eq!(context.get::<CustomValue>()?.0, 2);
    assert_eq!(context.get::<String>()?.as_str(), "ready");
    Ok(())
}
```

默认 marker 名称由函数名转成 PascalCase 后追加 `Bean`：`default_value` 对应
`DefaultValueBean`。`marker = CustomFactory` 可覆盖默认名称。返回
`Arc<CustomValue>` 时，容器保存的组件类型是 `CustomValue`，因此根节点和查询也用
`CustomValue`。`#[Configuration]` 模块导出 `register_ioc`；调用它只安装模块中的
bean，不执行工厂。这个分组功能不依赖 `config`，而通过 `#[value]` 或
`#[ConfigurationProperties]` 读取配置时，才需要同时启用 `macros` 和 `config`。

三个根节点选中相应定义；构建后的断言验证 `1`、`2` 和 `"ready"`。在仓库根目录运行
同一份[完整示例](../examples/readme_beans.rs)：

```bash
cargo +1.94.0 run --example readme_beans --no-default-features --features macros --locked
```

实际 provider crate 应导出注册入口，由消费应用在启动时调用。只有构造过程需要等待时，
才将 bean 声明为 `async fn`，并用应用执行器驱动 `builder.build_async().await?`。
选中图含异步工厂却调用同步构建时，会在运行任何工厂或 alias projector 前返回
`BuildError::AsyncRequired`；图验证失败同样不会启动构造。后续工厂仍可能失败，
此时先前完成的托管工厂所移交的资源会参与回滚。

bean 需要启动 worker 时，在工厂内部创建它，并返回
`Managed::asynchronous(value, abort, wait)`，让关闭流程确认 worker 终止。
只有 stop 成功返回就意味着资源已终止时，才使用
`Managed::synchronous(value, stop)`。选中托管图时先配置 bounded
`WaitPolicy`，并保留构建返回的 `Application`；
`application.context()` 的克隆在关闭期间也可存在。正常退出调用
`application.begin_shutdown(ShutdownMode::Graceful)` 并等待句柄的 `wait()`；
失败和取消走 Immediate abort。[生命周期说明](lifecycle.zh_CN.md)提供完整的
bounded 集成函数，在构建失败清理和正常关闭两条路径都显式等待；
[托管 worker 示例](../examples/app_lifecycle.rs)同样展示所有者和句柄契约。

## 类型化工厂与异步构建

手工同步和异步工厂都提供类型化注册入口：`register_injected_factory`、
`register_injected_managed_factory`、`register_injected_async_factory` 和
`register_injected_managed_async_factory`。参数元组会生成依赖请求：`()` 不请求依赖，
`Arc<T>` 要求一个绑定，`Option<Arc<T>>` 允许没有匹配项，`Vec<Arc<T>>` 请求全部候选；
参数数量支持从零到八个。若需按具名 ID 查找或使用自定义依赖请求，请显式声明请求，改用
`register_async_factory` 或 `register_managed_async_factory`。

只要所选构建图包含异步工厂，就要调用 `build_async()` 或 `build_all_async()`，并由应用
选定的执行器轮询。若此时调用同步 `build()`，会在任何工厂运行前返回
`BuildError::AsyncRequired`。条件允许时，应等依赖图验证通过后再创建托管资源；异步工厂
返回 `Managed<T>` 之前产生的副作用，若之后构建失败或被取消，仍由工厂自行处理。

下面的完整函数通过必需的 `String` 依赖创建异步工厂，并异步构建全部活跃定义：

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

`build_message_length()` 成功时查询结果为 `5`。若工厂在返回 `Managed<T>` 前已产生外部副作用，
容器尚未接管这些副作用，工厂仍需自行处理。

## 选择定义与构建范围

启用默认 feature 时，可以用 `#[Component]`、`#[Service]`、`#[Repository]` 声明
组件，用 `#[bean]` 声明工厂。调用 `builder.install::<T>()?` 显式安装定义，也可调用
提供者 crate 的 `register_ioc(&mut builder)` 组装入口。要把具体组件作为 trait 注入，
需写 `bind = dyn Trait`；独立的 `impl` 不会
自动生成绑定。`macros` 与 `config` 可独立启用：组件和 bean 声明需要 `macros`；
`#[value]`、`#[ConfigurationProperties]` 同时需要 `macros` 和 `config`，只开
`config` 不会导出这些宏。关闭默认 feature 后，手动注册不需要这两个 feature。
字段和 bean 参数上的 `cfg` 与嵌套 `cfg_attr(..., cfg(...))` 会同步控制生成的依赖请求、
字段初始化和函数调用实参。
`inject`、`value` 等 helper 必须直接写在组件字段或 bean 参数上；嵌在 `cfg_attr` 中会产生清晰诊断。

`root::<T>()` 选择未指定 ID 的根节点；`root_by_id::<T>("some.id")?` 精确
选择绑定。选定根节点后，`build()` 默认只验证根节点可达的传递依赖。
设置 `validation_scope(ValidationScope::AllActive)` 后，会先检查所有活跃定义，再只构造
根节点的传递依赖；`AllActive` 不会执行未选中的工厂，也不要求未选中的异步工厂或托管工厂
符合本次构建方式或 `WaitPolicy`。`build_all()` 会检查并构造所有活跃定义，也允许空图。
两种构建方式都有异步版本。所选构建图包含异步工厂时，应用需自行提供执行器并调用
`build_async()` 或 `build_all_async()`。

应用装配中心若要在发布前检查未选中但仍活跃的错误定义，可选用 `AllActive`；按 profile
或租户有意选择部分根闭包时保留默认 `Reachable`。下面未选中的定义缺少依赖：
`Reachable` 下根构建成功，`AllActive` 在任何工厂运行前拒绝构建：

```rust
use std::sync::Arc;
use qubit_ioc::{BuildError, ContainerBuilder, Dependency, ValidationScope};

fn builder() -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    builder.register_factory::<u32, _>(&[], |_| Ok(Arc::new(1)))
        .expect("register root");
    builder.register_factory::<u64, _>(&[Dependency::of::<i16>()], |_| Ok(Arc::new(2)))
        .expect("register unselected definition");
    builder.root::<u32>();
    builder
}

fn main() {
    assert!(builder().build().is_ok());
    let failure = builder().validation_scope(ValidationScope::AllActive)
        .build().err().expect("missing dependency");
    assert!(matches!(failure.cause(), BuildError::MissingDependency { .. }));
}
```

`build_all()` 用于真正构造所有活跃定义；此例也会因缺失依赖而失败。仅设置 `AllActive`
不会执行未选中工厂。

同一类型只有一个候选时，未指定 ID 的请求直接选中它；多个候选时，必须有唯一的
`primary`，否则返回歧义错误。精确选择需要有效 ID：各段以点分隔，首字符为
ASCII 字母，后续只能使用 ASCII 字母、数字或下划线。构建后可用
`get_by_id::<T>()`、`try_get::<T>()`、`get_all::<T>()` 分别进行精确、可选或
集合查询。`get_all()` 复用 context 发布时按 `order`、ID、来源位置和注册位置预排序的不可变索引。字段和 bean 参数上的 `cfg`（包括嵌套
`cfg_attr(..., cfg(...))`）会同步控制生成的依赖请求、字段初始化和工厂调用实参。

`register_instance_with` 暂存实例时会校验自身的 ID 和 profile；与其他定义的键冲突会在
构建时、过滤非活跃 profile 后检查。工厂 panic 按 Rust 的常规 panic 语义传播。若构造在
托管资源移交给 IoC 后失败，原始同步和异步构建入口都会请求 abort，并立即返回 `BuildFailure`，不会
等待清理完成。由持有该失败值的应用负责观察清理结果。包装成应用错误时，应
保留 `BuildFailure` 直到 `wait_cleanup()` 完成，或保留 `take_cleanup()` / `into_parts()`
返回的 `ShutdownHandle` 并等待它完成。只保留 `BuildError` 或 cause 会丢失清理观察句柄。
`wait_cleanup()` 借用失败值并保留原始原因；如果等待 future 被取消，可以再次调用继续观察。

定义可以指定生效的 `profile`。每次调用 `active_profiles` 都会替换此前的集合；空集合
激活 `default`，无 profile 定义始终生效。通过 `#[value]` 或
`#[ConfigurationProperties]` 读取配置，需要启用 `config` feature 并登记
`qubit-config` 的配置快照。重复的活跃 `Config` 键会在构建阶段返回
`BuildError::DuplicateBinding`。完整契约见[当前设计](complete-design.zh_CN.md)与
[English Current Design](complete-design.md)。

### 替换一个完整定义

覆盖测试或替换运行时实现时，回调可以捕获应用状态：

```rust
use std::error::Error;
use std::sync::Arc;
use qubit_ioc::{BindingKey, ContainerBuilder, Definition};

fn replace_for_test() -> Result<(), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(1_u64))?;
    let fake = Arc::new(7_u64);
    let key = BindingKey::of::<u64>(None);
    builder.replace_definition(key, move |draft| {
        let replacement = Definition::<u64>::builder().instance(fake).build()?;
        draft.register_definition(replacement)
    })?;
    builder.root::<u64>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(*context.get::<u64>()?, 7);
    Ok(())
}
```

回调会先在临时 builder 上执行。`Definition::builder()` 还可设置 `binding`、
`dependencies`、同步/异步及托管工厂，并通过 `bind` 声明 trait alias；
`build()` 不执行用户工厂，只验证完整定义，`register_definition` 则原子登记。
宏也使用这套公开核心，而不依赖已移除的隐藏定义协议。若回调返回错误，或没有为目标键恰好注册一个定义，
原 builder 不会改变。替换成功后原定义的全部键（包括 alias）都会移除；新定义需要重新声明仍需保留的 alias。

## 配置和依赖请求怎么选

某个 profile 可能不提供依赖时，用 `Option<Arc<T>>`；没有候选会得到 `None`。需要所有实现时，用 `Vec<Arc<T>>`；没有候选会得到空集合。只要这些请求命中生效定义，它们仍参与依赖图验证。需要固定使用某个实现时，在字段上写 `#[inject(id = "storage.primary")]`。指定 ID 不存在会在构建阶段报错，不会退回 `primary`。

同时启用 `macros` 和 `config` 后，可用 `#[value("service.port")]` 读取单个配置值。安装定义前通过 `builder.with_config(config)?` 登记配置快照。键缺失或类型不匹配会转成 `BuildError::ConfigReadFailed`，错误保留原始配置来源和字段路径。

读取一组结构化配置时，给具名字段结构体派生 `Deserialize`，并标注 `#[ConfigurationProperties(prefix = "service")]`。安装该定义，并让 builder 使用同一份配置快照。属性缺失或反序列化失败会中止构建，错误保留原始反序列化细节。`tests/config_macro_tests.rs` 展示了完整设置方式和成功结果。

`#[value]` 读取保存的原始值。结构化读取同样不会插值，并且默认拒绝未知字段。
应用需要插值时，可在手工工厂中注入 `Config` 并调用
`Config::get_interpolated`；可运行示例见
[`examples/config_contract.rs`](../examples/config_contract.rs)，对应测试见
[`config_tests.rs`](../tests/config_tests.rs)。

### Managed 工厂的返回路径

`#[bean]` 仅在 `Managed<T>` 路径符合运行时依赖 crate 名称时，才将其识别为托管输出：

| 运行时依赖名称 | 支持的返回路径 |
| --- | --- |
| `qubit-ioc` | 导入后的 `Managed<T>`、`qubit_ioc::Managed<T>`、`::qubit_ioc::Managed<T>` |
| 重命名为 `ioc` | `ioc::Managed<T>`、`::ioc::Managed<T>` |
| 重命名为关键字 `type` | `r#type::Managed<T>`、`::r#type::Managed<T>` |

同步和异步 bean 的 `Result<Managed<T>, E>` 也支持这些路径。宏不会猜测
`application::Managed<T>` 或类型别名就是运行时封装；它们仍按普通组件输出处理。
独立消费 workspace 在
[`managed_paths`](../tests/fixtures/managed_paths/) 中覆盖了每种路径。

## 跨 crate 装配：应用选择 provider

provider crate 负责组件定义；应用负责决定安装哪些 provider、构建哪些服务。仓库的 `tests/fixtures/ioc_cross_crate/` 展示了这个边界。这是可运行的契约测试，不代表已有生产部署采用该方案。

provider crate 导出 `register_ioc(&mut builder)`。应用夹具的 `app/src/discovery.rs` 提供 `assemble(config, profiles)`：创建 builder，登记配置快照并选择 profile，再调用 provider 的注册入口。登记只暂存定义；工厂要等到 `build()` 或 `build_all()` 才会运行。

消费方依赖关系见 `tests/fixtures/ioc_cross_crate/app/Cargo.toml`：应用直接依赖 `qubit-ioc`、`qubit-config`、provider crate 和 contracts crate。集成测试先创建 `Config` 并设置 `fixture.label`，再调用 `assemble(config, &[])` 构图，查询 `AppService`、按 ID 查询具体 repository，并查询 primary 的 `dyn Repository`。测试验证具体类型和 trait alias 指向同一份实例，也验证 provider crate 中的 bean 能读取应用登记的配置。运行这组契约测试：

```bash
cargo test --manifest-path tests/fixtures/ioc_cross_crate/Cargo.toml
```

若配置子树缺失，构造返回 `BuildFailure`，其 cause 为
`BuildError::ConfigReadFailed`，source 链保留原始 `ConfigError`。需要启用可选 preview provider 时，调用 `assemble(config, &["default", "preview"])`；未激活的定义不会出现在 context 中。夹具集成测试覆盖了这两种结果。

另一个下游夹具 `rs-execution-services/tests/fixtures/ioc_application_consumer/src/main.rs` 展示托管资源生命周期：安装托管的 `ExecutionServices` 与 `EventBus`，构建后取得共享服务，正常退出通过 `Application::begin_shutdown(ShutdownMode::Graceful)` 请求排空，
失败走 `Immediate`，再等待 `ShutdownHandle::wait()`。EventBus adapter 使用
`request_shutdown`、ticket 和 `wait_async()`；同步 `EventBus::shutdown(Immediate)`
仍会在停止回调中等待。这些片段来自不同夹具、承担不同验证目标；应用仍需自行处理配置来源和外部副作用。

只有构造过程需要等待 I/O 时才使用异步工厂。`#[bean] async fn`、
`register_injected_async_factory` 和 `register_injected_managed_async_factory`
适用于类型化依赖元组；需要按 ID 查找或设置自定义请求时，改用
`register_async_factory` 或 `register_managed_async_factory`。之后调用
`build_async()` 或 `build_all_async()`，并由应用执行器驱动 future。若选中图里有异步定义却调用
同步 `build()`，会在工厂运行前返回 `BuildFailure`，其 cause 为
`BuildError::AsyncRequired`。执行器选择和取消策略由应用负责。



## 构建失败与回滚观察

构建图验证失败时，工厂尚未运行，因此没有托管资源需要回滚。若某个工厂在前序托管资源已创建后失败，原始 `build()` 或 `build_async()` 会请求这些资源 abort，随即返回 `BuildFailure`；调用方可以立即处理启动失败，也可以保留该值并等待清理。只保留 `BuildError` 或 `cause()` 会丢失清理观察句柄。

下面让 `Worker` 成功创建后再让 `Startup` 失败，并分别演示清理成功与清理失败。它沿用 README 的完整示例，使用 settled 构建等待正常完成的回滚观察：

```rust
use std::error::Error;
use std::sync::Arc;
use qubit_ioc::{
    CleanupError, ContainerBuilder, Dependency, FactoryError, Managed,
    SettledBuildFailure, WaitPolicy,
};

struct Worker;
struct Startup;

async fn fail_after_worker(
    fail_cleanup: bool,
) -> Result<(SettledBuildFailure, bool), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder.register_managed_factory::<Worker, _>(&[], move |_| {
        Ok(Managed::synchronous(Arc::new(Worker), move |_| {
            if fail_cleanup {
                Err(CleanupError::new(std::io::Error::other("cleanup failed")))
            } else {
                Ok(())
            }
        }))
    })?;
    builder.register_factory::<Startup, _>(&[Dependency::of::<Worker>()], |_| {
        Err(FactoryError::new(std::io::Error::other("startup failed")))
    })?;
    builder.root::<Startup>();

    let failure = match builder.build_settled().await {
        Ok(_) => unreachable!("the Startup factory always fails"),
        Err(failure) => failure,
    };
    let cleanup_succeeded = failure
        .cleanup_report()
        .expect("Worker was constructed before Startup failed")
        .is_success();
    Ok((failure, cleanup_succeeded))
}

async fn show_cleanup_reports() -> Result<(), Box<dyn Error>> {
    let (failure, cleanup_succeeded) = fail_after_worker(false).await?;
    assert!(cleanup_succeeded);
    println!("build cause: {}", failure.cause());

    let (failure, cleanup_succeeded) = fail_after_worker(true).await?;
    assert!(!cleanup_succeeded);
    eprintln!("build cause: {}; cleanup report: {:?}", failure.cause(), failure.cleanup_report());
    // 检查报告后，应用仍可通过 Err(Box::new(failure)) 返回原始失败。
    Ok(())
}
```

此例中，`Startup` 的构建错误仍是原始失败原因；`cleanup_report()` 因 `Worker` 已创建而存在，且分别显示清理成功、失败。真实应用中，`cleanup_report()` 是可选值：图或预检失败没有发生托管清理，不能假设一定有报告；清理报告也可能表示失败或未完整确认。`SettledBuildFailure::cause()` 保留原始 `BuildError`，报告用于观察回滚结果，不会取代错误来源。

四个 settled 入口都是 async 方法，返回 `Result<Application, SettledBuildFailure>`。仅含同步工厂的图可使用 `build_settled()` 或 `build_all_settled()`（仍须 `.await`）；包含异步工厂时使用 `build_async_settled()` 或 `build_all_async_settled()`。正常完成时，它们会在返回失败前等待回滚观察结束。需要尽快拿到失败并由应用自行安排清理时，使用原始入口；`BuildFailure::wait_cleanup()` 借用失败值并等待，若等待 future 被取消，可再次调用继续观察。也可通过 `take_cleanup()` 或 `into_parts()` 转移清理所有权并等待句柄。

取消或丢弃 settled future 会中断等待，只会尽力请求 abort，并不保证回滚观察完成。若使用原始入口，保留 `BuildFailure` 才能在取消后继续调用 `wait_cleanup()`。计时器也不能抢占同步阻塞回调或单次阻塞的 future poll。

### 构建期间取消后继续观察清理

如果应用需要在启动被关闭信号打断后继续观察异步构建清理，请使用
`build_async_session()` 或 `build_all_async_session()`，并保留返回的 `BuildSession`。
普通 `build_async()` 的 future 被取消时，应用拿不到可恢复观察清理的 session。
下面假定 `builder` 已配置完成，`shutdown_signal` 是应用提供、输出 `()` 的关闭信号
future；`tokio::select!` 只是 Tokio 应用的选择写法示例，`qubit-ioc` 不绑定 Tokio
或任何其他异步执行器。

```rust
use qubit_ioc::{BuildSessionError, ShutdownMode};

let mut session = builder.build_async_session();
let outcome = tokio::select! {
    result = session.run() => Some(result),
    () = shutdown_signal => None,
};
let application = match outcome {
    Some(Ok(application)) => Some(application),
    Some(Err(BuildSessionError::Build(mut failure))) => {
        let report = failure.wait_cleanup().await;
        eprintln!("build failed: {}; cleanup: {report:?}", failure.cause());
        None
    }
    Some(Err(other)) => {
        eprintln!("build session state: {other}");
        None
    }
    None => {
        let report = session.wait_cancelled_cleanup().await;
        eprintln!("startup cancelled; cleanup: {report:?}");
        None
    }
};
if let Some(application) = application {
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    eprintln!("shutdown: {:?}", shutdown.wait().await);
}
```

`select!` 的关闭信号分支胜出时，会丢弃 `session.run()` future；必须等这个 future
结束并释放对 session 的借用后，才能调用 `wait_cancelled_cleanup()`。应用随后应检查
其 `Option<ShutdownReport>`：`Some(report)` 表示取得清理报告，但报告仍可能包含失败或
未确认终止项；`None` 表示 session 当前没有清理句柄，例如尚无托管资源完成构造，或
句柄已移交。等待 future 若再次被取消，可再次调用 `wait_cancelled_cleanup()`，继续观察
同一清理过程和原有期限。

还需区分这些结果：`Some(Err(BuildSessionError::Build(failure)))` 表示构建已返回失败，
清理所有权在 `failure` 中，应调用它的 `wait_cleanup()`，而不是从 session 取清理；
构建成功则由返回的 `Application` 接管资源，应用仍须执行正常关闭。若 `run()` 尚未
首次轮询就被丢弃，session 仍可再次运行；首次轮询后被取消，再次 `run()` 会返回
`BuildSessionError::Cancelled`，无需用它来获取清理句柄；成功或失败结果已经返回后再
运行则返回 `BuildSessionError::AlreadyFinished`。需要把取消后的清理交给其他任务时，
可调用 `take_cancelled_cleanup()`：它至多返回一次 `ShutdownHandle`，之后等待责任归
接收方。直接丢弃 session 只会尽力请求 abort，不会等待清理报告。计时期限也不能抢占
同步阻塞回调或单次阻塞的 future poll。

## 托管关闭预算

当应用需要给托管资源的正常停止和终止确认设置统一上限时，可配置
`WaitPolicy::bounded_with_total(grace, termination, total, timer)`。总计时从
`ShutdownHandle::wait()` 首次被 poll 时启动；取消等待后预算仍保留。到期后，系统会请求尚未确认终止的组件 abort，并通过 `ShutdownReport::overall_failure()` 报告总体失败。应用应检查关闭报告，并按业务策略处理仍未确认终止的组件；期限不能中断同步阻塞工作或阻塞的单次 future poll。详见[生命周期说明](lifecycle.zh_CN.md)和[托管资源适配指南](managed-adapters.zh_CN.md)。

## 错误与排障

| 症状 | 检查位置 | 处理方式 |
| --- | --- | --- |
| `RegistrationError::InvalidBindingId` 或 `DuplicateDependency` | 注册调用或宏声明 | 修正 ID 格式或重复的依赖声明。 |
| `BuildError::NoRootsSelected` 或 `MissingRoot` | 根节点选择 | 增加根节点，或确认绑定已生效。 |
| `BuildError::MissingDependency`、`AmbiguousBinding` 或 `DependencyCycle` | 依赖路径与候选绑定 | 补齐依赖、指定 ID 或主候选，或解除环路。 |
| `BuildError::AsyncRequired` | 构建方法 | 由应用执行器驱动异步构建。 |
| `BuildError::FactoryFailed` 或 `ConfigReadFailed` | 原始错误及构建路径 | 修复工厂或配置输入。 |
| `ResolveError::MissingComponent` 或 `AmbiguousBinding` | 查询类型及候选绑定 | 查询已构建的根节点，或使用 `get_by_id()`。 |

依赖图错误会在用户工厂运行前返回。后续工厂失败时，容器会清理已移交的托管资源；
其余外部副作用由工厂或应用自行处理。可沿 `FactoryFailed` 保留的错误来源检查原因。
工厂只能读取登记时声明的依赖，
否则会得到 `BuildAccessError::UndeclaredDependency`。

## 生命周期与限制

`Application` 持有生命周期所有权，`application.context()` 提供只读共享查询句柄。
可克隆句柄分发给并发读者；关闭时不需要收回每个克隆或执行 `Arc::try_unwrap`。
[上下文共享示例](../examples/context_sharing.rs)展示这个分工。关闭开始后查询仍可能
成功。`ApplicationContext::state()` 用于观察生命周期进度，不会限制上下文查询；查询成功
不表示服务继续接收业务工作。

托管资源应在图验证通过后由托管工厂创建。已运行的外部资源用
`register_instance(Arc<T>)` 注入，应用自己负责关闭。真实应用的托管图应配置
bounded `WaitPolicy`。stop 成功返回即确认终止时使用 `Managed::synchronous`；
需要先非阻塞请求 abort、再等待终止时使用 `Managed::asynchronous`。需要排空时，构造阶段
选择 `Managed::synchronous_with_graceful` 或 `Managed::asynchronous_with_graceful`；ticket
构造器则负责把请求结果交给对应的 wait 回调。构造后 graceful 回调不可替换。具体选择见
[托管资源适配指南](managed-adapters.zh_CN.md)。正常退出选 Graceful 并等待
句柄；Graceful 从首次轮询 `wait` 才开始逐个请求，消费者终止后才处理其依赖。
Immediate 在返回句柄前向全部托管组件请求 abort。[生命周期说明](lifecycle.zh_CN.md)
包含构建失败和业务退出的完整流程，[app_lifecycle](../examples/app_lifecycle.rs)
提供实际 worker 示例。

原始同步和异步构建入口失败时都会先请求已移交资源 abort，再立即返回 `BuildFailure`；等待和报告
由持有失败值的应用负责。应用检查 `cause()`，可用 `wait_cleanup()` 等待可选清理，也可用
`take_cleanup()` 或 `into_parts()` 取出句柄后等待。包装成应用错误时，在
使用原始构建入口时，应保留 `BuildFailure` 直到清理完成，或保留提取出的 `ShutdownHandle` 并等待它完成。只保留
`BuildError` 或 cause 会丢失清理观察句柄。`wait_cleanup()` 借用失败对象；取消后再次调用会继续
等待同一清理过程。丢弃 owner、未移交的 `Managed` 或关闭句柄会请求 abort，但不等待；查询
context Drop 不请求关闭。丢弃构建失败对象或取消异步构建也不等待。工厂 unwind
panic 仍向外传播；工厂在返回 `Managed` 前产生的副作用由它自身负责。

取消 `ShutdownHandle::wait()` 后，同一句柄会保留当前 future 与期限，再次调用会
接着执行。bounded 期限要求应用驱动计时器，无法打断阻塞回调或阻塞的 future poll。
`ShutdownReport::incomplete()` 表示终止未获确认，并非资源已被杀死。即使全部
wait 完成，报告仍可能保留回调错误；外部 `Arc` 克隆可在关闭后继续持有对象内存。

旧托管实例注册入口已移除；使用 `register_managed_factory` 或托管 `#[bean]` 工厂。
`Definition::builder()` 和 `register_definition` 提供公开的完整自定义注册与 trait
alias 能力。集合查询复用发布时预排序的索引；构建和清理顺序与此独立。

当前不提供原型或请求作用域、热更新、未托管组件自动关闭、循环代理或动态库发现。
结构体宏支持具名字段和单元结构体；其他形状可用手动工厂。组件构造不依赖运行时
反射，provider 选择与回退由 `qubit-spi` 另行负责。

继续阅读[项目 README](../README.zh_CN.md)、[English user guide](user_guide.md)，
或运行 `cargo doc --no-deps --open` 查看 API 文档。
