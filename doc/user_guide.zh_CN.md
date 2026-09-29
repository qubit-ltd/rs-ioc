# qubit-ioc 用户手册

[English user guide](user_guide.md) · [项目 README](../README.zh_CN.md)

本手册面向使用 `qubit-ioc` 0.2.0 发布候选的 Rust 应用开发者，介绍如何在启动时组装
应用级共享组件、定位错误，以及安排资源关闭。项目清单要求 Rust 1.94 或更新版本。

## 概念模型

| 术语 | 含义 |
| --- | --- |
| 定义 | 已登记的实例或工厂；登记时不会执行工厂。 |
| 绑定 | 由 Rust 类型和可选的区分大小写 ID 标识的组件。 |
| 依赖 | 工厂显式声明或宏根据字段生成的请求。 |
| 根节点 | `build()` 要构建的组件及其传递依赖的起点。 |
| 上下文 | 成功构建后发布的只读 `ApplicationContext`。 |

构建器先筛选生效的 profile，再解析根节点和依赖、验证依赖图，最后按依赖顺序执行
工厂。`build_all()` 则构建所有生效的定义。构建失败时不会发布部分上下文。

## 跨 crate 装配：应用选择 provider

provider crate 负责组件定义；应用负责决定安装哪些 provider、构建哪些服务。仓库的 `tests/fixtures/ioc_cross_crate/` 展示了这个边界。这是可运行的契约测试，不代表已有生产部署采用该方案。

provider crate 导出 `register_ioc(&mut builder)`。应用夹具的 `app/src/discovery.rs` 提供 `assemble(config, profiles)`：创建 builder，登记配置快照并选择 profile，再调用 provider 的注册入口。登记只暂存定义；工厂要等到 `build()` 或 `build_all()` 才会运行。

消费方依赖关系见 `tests/fixtures/ioc_cross_crate/app/Cargo.toml`：应用直接依赖 `qubit-ioc`、`qubit-config`、provider crate 和 contracts crate。集成测试先创建 `Config` 并设置 `fixture.label`，再调用 `assemble(config, &[])` 构图，查询 `AppService`、按 ID 查询具体 repository，并查询 primary 的 `dyn Repository`。测试验证具体类型和 trait alias 指向同一份实例，也验证 provider crate 中的 bean 能读取应用登记的配置。运行这组契约测试：

```bash
cargo test --manifest-path tests/fixtures/ioc_cross_crate/Cargo.toml
```

若配置子树缺失，构造返回 `BuildError::ConfigReadFailed`，source 链保留原始 `ConfigError`。需要启用可选 preview provider 时，调用 `assemble(config, &["default", "preview"])`；未激活的定义不会出现在 context 中。夹具集成测试覆盖了这两种结果。

另一个下游夹具 `rs-execution-services/tests/fixtures/ioc_application_consumer/src/main.rs` 展示托管资源生命周期：安装托管的 `ExecutionServices` 与 `EventBus`，构建后取得共享服务，在退出时调用 `begin_shutdown()` 请求停止，再等待 `ShutdownHandle::wait()` 完成。这些片段来自不同夹具、承担不同验证目标；应用仍需自行处理配置来源和外部副作用。

## 场景：用共享配置启动服务

一个应用有问候文本，服务启动时需要读取它。目标是在启动阶段构建服务，看到输出，
并确认多次查询取得同一个实例。下面使用手动注册；即使关闭全部默认 feature，
这条路径也可用。

### 安装与最小配置

在与本仓库相邻的应用中添加依赖：

```toml
[dependencies]
qubit-ioc = { version = "0.2", path = "../rs-ioc", default-features = false }
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

    let context = builder.build()?;
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

## 选择定义与构建范围

启用默认 feature 时，可以用 `#[Component]`、`#[Service]`、`#[Repository]` 声明
组件，用 `#[bean]` 声明工厂。调用 `builder.install::<T>()?` 显式安装定义，也可调用
提供者 crate 的 `register_ioc(&mut builder)` 组装入口。要把具体组件作为 trait 注入，
需写 `bind = dyn Trait`；独立的 `impl` 不会
自动生成绑定。`macros` 与 `config` 可独立启用：组件和 bean 声明需要 `macros`；
`#[value]`、`#[ConfigurationProperties]` 同时需要 `macros` 和 `config`，只开
`config` 不会导出这些宏。关闭默认 feature 后，手动注册不需要这两个 feature。
字段上的 `cfg` 与嵌套 `cfg_attr(..., cfg(...))` 会同步控制生成的依赖请求和字段初始化。
`inject`、`value` 等 helper 必须直接写在字段上；嵌在 `cfg_attr` 中会产生清晰诊断。

`root::<T>()` 选择未指定 ID 的根节点；`root_by_id::<T>("some.id")?` 精确
选择绑定。`build()` 至少需要一个根节点，只构建它的传递依赖；`build_all()` 会
构建所有生效定义，也允许空图。两者都有异步版本。包含异步工厂时，应用需自行
提供执行器并调用 `build_async()` 或 `build_all_async()`。

同一类型只有一个候选时，未指定 ID 的请求直接选中它；多个候选时，必须有唯一的
`primary`，否则返回歧义错误。精确选择需要有效 ID：各段以点分隔，首字符为
ASCII 字母，后续只能使用 ASCII 字母、数字或下划线。构建后可用
`get_by_id::<T>()`、`try_get::<T>()`、`get_all::<T>()` 分别进行精确、可选或
集合查询。`get_all()` 按 `order`、ID 和来源位置排序。

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
use qubit_ioc::{BindingKey, ContainerBuilder};

fn replace_for_test() -> Result<(), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(1_u64))?;
    let fake = Arc::new(7_u64);
    let key = BindingKey::of::<u64>(None);
    builder.replace_definition(key, move |draft| draft.register_instance(fake))?;
    builder.root::<u64>();
    let context = builder.build()?;
    assert_eq!(*context.get::<u64>()?, 7);
    Ok(())
}
```

回调会先在临时 builder 上执行。若回调返回错误，或没有为目标键恰好注册一个定义，
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

只有构造过程需要等待 I/O 时才使用异步工厂。可以声明 `#[bean] async fn`，也可调用 `register_async_factory`；之后用 `build_async()` 或 `build_all_async()`，并由应用执行器驱动 future。若选中图里有异步定义却调用同步 `build()`，会在工厂运行前返回 `BuildError::AsyncRequired`。执行器选择和取消策略由应用负责。

## 错误与排障

| 症状 | 检查位置 | 处理方式 |
| --- | --- | --- |
| `RegistrationError::InvalidBindingId` 或 `DuplicateDependency` | 注册调用或宏声明 | 修正 ID 格式或重复的依赖声明。 |
| `BuildError::NoRootsSelected` 或 `MissingRoot` | 根节点选择 | 增加根节点，或确认绑定已生效。 |
| `BuildError::MissingDependency`、`AmbiguousBinding` 或 `DependencyCycle` | 依赖路径与候选绑定 | 补齐依赖、指定 ID 或主候选，或解除环路。 |
| `BuildError::AsyncRequired` | 构建方法 | 由应用执行器驱动异步构建。 |
| `BuildError::FactoryFailed` 或 `ConfigReadFailed` | 原始错误及构建路径 | 修复工厂或配置输入。 |
| `ResolveError::MissingComponent` 或 `AmbiguousBinding` | 查询类型及候选绑定 | 查询已构建的根节点，或使用 `get_by_id()`。 |

依赖图错误会在用户工厂运行前返回。工厂失败前已经发生的外部副作用不会自动撤销；
可沿 `FactoryFailed` 保留的错误来源检查原因。工厂只能读取登记时声明的依赖，
否则会得到 `BuildAccessError::UndeclaredDependency`。

## 生命周期与限制

上下文保存共享 `Arc`，查询不会再次执行工厂，并可在多线程中并发查询。
需要关闭时先释放全部共享上下文句柄，再用 `Arc::try_unwrap` 取回唯一所有者，
随后调用 `begin_shutdown()`。完整示例见
[`examples/context_sharing.rs`](../examples/context_sharing.rs)。需要关闭的资源组件可显式选择
`Managed<T>`。仓库提供可运行的 worker 示例：它在托管工厂中启动任务，发送停止信号，
等待任务退出并检查结果。可在仓库根目录运行
`cargo run --example app_lifecycle --no-default-features`；源码见
[`examples/app_lifecycle.rs`](../examples/app_lifecycle.rs)，关闭和取消语义见
[生命周期说明](lifecycle.zh_CN.md)。

托管资源应在托管工厂中创建；工厂只会在依赖图验证后运行。装配前已启动的外部资源
使用 `register_instance(Arc<T>)` 注入，关闭动作由应用负责。不要在托管工厂闭包中捕获
已创建的 `Managed<T>`。

`Managed::new` 接收同步 stop 请求，`.with_wait` 可添加异步终止等待。
`begin_shutdown(self)` 在返回前按实际构建顺序的逆序调用所有 stop。返回的
`ShutdownHandle::wait(&mut self)` 按同一顺序等待并返回所有清理错误。若 wait future
被取消，保留句柄并再次调用 `wait()` 可继续同一个 future。同步 `build()` 失败时只停止已完成的托管资源，不会等待；
异步 `build_async()` 失败时先 stop 再 wait，并把清理错误与原始构建错误一起保留。
若构建失败时也必须等已有资源终止，即使所有工厂同步，也使用 `build_async()`。
异步构建 future 被取消时只调用 stop，
不等待；此时调用方已无法接收清理错误。工厂在返回 `Managed<T>` 前产生的副作用
由工厂自身负责收尾。

普通释放上下文不会自动停止资源。外部持有的 `Arc` 可使对象在
关闭后继续存活；关闭动作不能撤销这些克隆。stop 回调 panic 会作为 Stop 阶段失败
记录。wait 回调创建或 wait future 轮询时发生的 unwind panic 会作为 Wait 阶段失败
记录，其他 wait 仍会执行。`panic = "abort"` 和 future 析构时的 panic 不会被捕获。
构造工厂 panic 仍会传播。

`register_managed_instance` 和 `DefinitionDraft::from_managed_instance` 已移除。资源应在
`register_managed_factory` 或 `DefinitionDraft::new_managed_sync` 中创建；若资源必须在
注册前创建，则通过 `register_instance(Arc<T>)` 注入，并由应用自行处理关闭。

当前不提供原型或请求作用域、热更新、未托管组件的自动生命周期管理、循环代理和动态库发现。
结构体宏支持具名字段和单元结构体；其他形状可使用手动工厂。组件构造不使用运行时
反射。provider 的选择和回退由 `qubit-spi` 另行负责。

继续阅读[项目 README](../README.zh_CN.md)、[English user guide](user_guide.md)
或运行 `cargo doc --no-deps --open` 查看 API 文档。
