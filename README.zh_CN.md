# qubit-ioc

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc.svg?color=blue)](https://crates.io/crates/qubit-ioc)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

`qubit-ioc` 帮助 Rust 应用开发者在启动时组装跨 crate 的共享组件。它先验证依赖，
再执行工厂，让缺失或歧义的服务在启动阶段暴露，而不是留到请求处理时。构建成功后
可通过只读的 `ApplicationContext` 获取组件；既能用过程宏声明，也能手动注册。

## 安装

当前源码版本是 `0.1.0`。在本地源码工作区中可使用路径依赖：

```toml
[dependencies]
qubit-ioc = { version = "0.1", path = "../rs-ioc" }
```

默认启用 `macros`、`inventory` 和 `config`；只使用手动注册时可关闭默认 feature：

```toml
qubit-ioc = { version = "0.1", path = "../rs-ioc", default-features = false }
```

| feature | 默认 | 用途 |
| --- | --- | --- |
| `macros` | 开 | 提供 `#[Component]`、`#[Service]`、`#[Repository]`、`#[Configuration]`、`#[ConfigurationProperties]` 和 `#[bean]`。 |
| `inventory` | 开 | 通过 `discover()` 发现已链接 crate 的定义。 |
| `config` | 开 | 注册 `qubit-config` 快照，并使用 `#[value]`、`#[ConfigurationProperties]`。 |

## 快速开始：组装问候服务

假设应用需要跨 crate 注入问候服务。先声明具体组件及其 trait 绑定，再让服务通过
`Arc<dyn Trait>` 请求依赖，并把服务选为构建根节点。
由于 trait 的 `impl` 位于独立语法项，组件宏不会自动枚举它，因此需要写明
`bind = dyn Greeting`。

```rust
use std::sync::Arc;
use qubit_ioc::{ApplicationContext, Component, Service};

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
    let mut builder = ApplicationContext::builder().discover()?;
    builder.root::<Greeter>();
    let context = builder.build()?;
    assert_eq!(context.get::<Greeter>()?.greeting.text(), "hello");
    Ok(())
}
```

运行 `cargo run --example readme_declarative` 可执行此示例，完整源码见
[`examples/readme_declarative.rs`](examples/readme_declarative.rs)。`build()` 至少需要一个
选定的根节点；若要构造所有已注册定义，请使用 `build_all()`。

断言能观察到选中的实现。构建器会在创建组件前验证该服务的依赖链。

## 核心能力

字段和 bean 参数可使用 `#[inject(id = "...")]` 精确选择绑定；
`Option<Arc<T>>` 表示可缺省，`Vec<Arc<T>>` 注入全部候选。`#[bean]` 可标注同步或
异步自由函数；图中含异步工厂时须调用 `build_async()`。

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
    let context = builder.build()?;
    assert_eq!(*context.get::<usize>()?, 5);
    Ok(())
}
```

运行 `cargo run --example readme_manual --no-default-features` 可执行此示例，完整源码见
[`examples/readme_manual.rs`](examples/readme_manual.rs)。

绑定身份由 Rust 类型和可选 ID 共同决定。ID 使用 `example.greeting.english` 这样
以点分隔的 ASCII 段；每段以字母开头，后续可包含字母、数字或下划线。未指定 ID 的
请求会选择唯一候选，或多个候选中唯一标记为 `primary` 的绑定。

不适合用宏或静态发现时，可手动注册。构建根节点、profile、候选选择、错误处理和
关闭责任详见[用户手册](doc/user_guide.zh_CN.md)。下游应用装配示例见
[`rs-execution-services` 消费者夹具](https://github.com/qubit-ltd/rs-execution-services/blob/main/tests/fixtures/ioc_application_consumer/README.md)。
在 `rs-execution-services` 仓库根目录运行
`cargo run --manifest-path tests/fixtures/ioc_application_consumer/Cargo.toml`。
该夹具用于验证跨 crate 契约，不代表已有生产应用采用。

## 限制

当前只提供应用级共享实例，不提供原型或请求作用域、热更新、未托管组件的自动关闭、
循环代理和动态库发现。托管工厂可通过 `Managed<T>` 和
`ApplicationContext::shutdown_async` 显式注册 stop/wait 动作。结构体宏支持具名字段和单元结构体，其他形状可使用手动工厂。
组件构造不依赖运行时反射。`qubit-spi` 继续负责 provider 的选择和回退；其注册表或
选中的服务可作为普通 IoC 组件注册。完整 API 与诊断规则见
[完整设计文档](doc/complete-design.zh_CN.md)。

## 延伸阅读

按[中文用户手册](doc/user_guide.zh_CN.md)或[English user guide](doc/user_guide.md)
完成安装、构建、诊断和关闭流程。在源码目录运行 `cargo doc --no-deps --open`
可查看公开 API 文档。

## 测试

```bash
# 使用默认 feature 集运行测试
cargo test

# 使用项目声明的全部 feature 运行测试
cargo test --all-features

# 运行项目 CI 检查
./ci-check.sh

# 检查代码覆盖率
./coverage.sh
```

## 许可证

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

本项目基于 Apache License 2.0 授权。完整许可证文本请参阅
[LICENSE](LICENSE)。

## 贡献

欢迎贡献。请遵循 Rust API 指南，及时更新公共 API 文档与测试，并在提交
Pull Request 前运行 `./align-ci.sh`格式化代码，运行`./ci-check.sh`对齐CI要求。

## 作者

**Haixing Hu** - *Qubit Co. Ltd.*

仓库地址：[https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)
