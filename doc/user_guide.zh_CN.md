# qubit-ioc 用户手册

[English user guide](user_guide.md) · [项目 README](../README.zh_CN.md)

本手册面向使用 `qubit-ioc` 0.1.0 源码的 Rust 应用开发者，介绍如何在启动时组装
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

## 场景：用共享配置启动服务

一个应用有问候文本，服务启动时需要读取它。目标是在启动阶段构建服务，看到输出，
并确认多次查询取得同一个实例。下面使用手动注册；即使关闭全部默认 feature，
这条路径也可用。

### 安装与最小配置

在与本仓库相邻的应用中添加依赖：

```toml
[dependencies]
qubit-ioc = { version = "0.1", path = "../rs-ioc", default-features = false }
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
自动生成绑定。

`root::<T>()` 选择未指定 ID 的根节点；`root_by_id::<T>("some.id")?` 精确
选择绑定。`build()` 至少需要一个根节点，只构建它的传递依赖；`build_all()` 会
构建所有生效定义，也允许空图。两者都有异步版本。包含异步工厂时，应用需自行
提供执行器并调用 `build_async()` 或 `build_all_async()`。

同一类型只有一个候选时，未指定 ID 的请求直接选中它；多个候选时，必须有唯一的
`primary`，否则返回歧义错误。精确选择需要有效 ID：各段以点分隔，首字符为
ASCII 字母，后续只能使用 ASCII 字母、数字或下划线。构建后可用
`get_by_id::<T>()`、`try_get::<T>()`、`get_all::<T>()` 分别进行精确、可选或
集合查询。`get_all()` 按 `order`、ID 和来源位置排序。

定义可以指定生效的 `profile`。未主动选择时，默认 profile 生效；构建前可用
`active_profiles(&["name"])?` 指定其他 profile。通过 `#[value]` 或
`#[ConfigurationProperties]` 读取配置，需要启用 `config` feature 并登记
`qubit-config` 的配置快照。其余选项可用 `cargo doc --no-deps --open` 查看
公开 API，或阅读[完整设计](complete-design.zh_CN.md)。

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

上下文保存共享 `Arc`，查询不会再次执行工厂。需要关闭的资源组件可显式选择
`Managed<T>`：

```rust
use std::sync::Arc;
use qubit_ioc::{CleanupError, ContainerBuilder, Managed};

struct Worker;
impl Worker { fn request_stop(&self) -> Result<(), std::io::Error> { Ok(()) } }

async fn managed_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_managed_factory::<Worker, _>(&[], |_| {
        let worker = Arc::new(Worker);
        Ok(Managed::new(Arc::clone(&worker), |worker| {
            worker.request_stop().map_err(CleanupError::new)
        }))
    })?;
    builder.root::<Worker>();
    let context = builder.build_async().await?;
    let mut shutdown = context.begin_shutdown();
    shutdown.wait().await?;
    Ok(())
}
```

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
关闭后继续存活；关闭动作不能撤销这些克隆。工厂 panic 按 Rust 机制传播。

当前不提供原型或请求作用域、热更新、未托管组件的自动生命周期管理、循环代理和动态库发现。
结构体宏支持具名字段和单元结构体；其他形状可使用手动工厂。组件构造不使用运行时
反射。provider 的选择和回退由 `qubit-spi` 另行负责。

继续阅读[项目 README](../README.zh_CN.md)、[English user guide](user_guide.md)
或运行 `cargo doc --no-deps --open` 查看 API 文档。
