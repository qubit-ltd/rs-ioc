# qubit-ioc

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc.svg?color=blue)](https://crates.io/crates/qubit-ioc)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

`qubit-ioc` 面向需要在应用启动阶段组装跨 crate 共享组件的 Rust 开发者。它会先检查依赖关系，再运行工厂，让缺失或有歧义的服务尽早暴露；组装完成后由 `Application` 持有组件生命周期，并通过可克隆的只读 `ApplicationContext` 提供查询。组件既可用过程宏声明，也可手动注册。

## 安装

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
qubit-ioc = "0.3"
```

默认启用 `macros` 和 `config`。只使用手动运行时时，可在依赖声明中设置 `default-features = false`。Cargo 会合并各依赖路径启用的 feature；只要其他依赖仍启用默认 feature，它们最终就会生效。

| feature | 默认 | 用途 |
| --- | --- | --- |
| `macros` | 开 | 提供 `#[Component]`、`#[Service]`、`#[Repository]`、`#[Configuration]`、`#[ConfigurationProperties]` 和 `#[bean]`。 |
| `config` | 开 | 注册 `qubit-config` 快照；配置属性宏还需要 `macros`。 |

`macros` 和 `config` 可分别启用；`#[value]` 与 `#[ConfigurationProperties]` 需要同时启用二者。关闭默认 feature 后仍可使用手动注册 API，参见[手动组装示例](examples/readme_manual.rs)。

## 快速开始：组装问候服务

这个例子把一个问候实现绑定到 trait，再将它注入服务。由于 trait 的 `impl` 是独立语法项，组件宏不会自动发现该绑定，因此在 `#[Component]` 上显式写出 `bind = dyn Greeting`。

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

运行 `cargo run --example readme_declarative` 可执行此示例；完整源码见[`examples/readme_declarative.rs`](examples/readme_declarative.rs)。`build()` 至少需要一个选定的根节点；需要构造所有活跃定义时，可使用 `build_all()`。

应用装配检查可设置 `validation_scope(ValidationScope::AllActive)`，在发布所选依赖图前验证所有活跃定义，但仍只构造根节点的依赖闭包；未选中的工厂不会运行，其异步或托管工厂也不要求本次使用异步构建或 `WaitPolicy`。只需验证当前所选闭包时，保留默认的 `Reachable`。`build_all()` 则会实际构造所有活跃定义。

## 核心能力

- **显式装配与依赖选择：** 手动注册实例和工厂，或用宏声明组件；依赖可按类型、ID 或自定义请求选择。`Option<Arc<T>>` 表示可选依赖，`Vec<Arc<T>>` 获取全部候选。
- **配置与函数 bean：** `#[bean]` 支持同步、异步和托管工厂；`#[Configuration]` 可将 bean 组织成注册组。配置属性和完整示例见[函数 bean 与配置组场景](doc/user_guide.zh_CN.md#场景函数-bean-与配置组)。
- **异步构建：** 手动类型化工厂可通过 `register_injected_async_factory` 等入口声明，含异步工厂的依赖图需使用 `build_async()` 或 `build_all_async()`。完整用法见[用户手册](doc/user_guide.zh_CN.md)。
- **应用级关闭：** 托管资源由 `Managed<T>` 描述关闭动作；应用必须显式等待 `ShutdownHandle::wait()` 才能观察关闭结果。取消等待不会完成关闭，期限也无法抢占同步阻塞回调或单次阻塞的 future poll。策略和适配方式见[生命周期指南](doc/lifecycle.zh_CN.md)及[托管资源适配指南](doc/managed-adapters.zh_CN.md)。

## 构建失败与资源边界

原始 `build()` / `build_async()` 会在请求 abort 后立即返回 `BuildFailure`，适合需要尽快取得失败并自行安排清理的场景；四个 settled 构建入口会在 future 正常完成时等待回滚观察。两种方式的错误来源与清理报告边界，以及取消后的处理方法，详见[构建失败与回滚观察](doc/user_guide.zh_CN.md#构建失败与回滚观察)。若构建 future 本身可能被取消，且应用仍需观察清理，请保留由 `build_async_session()` 或 `build_all_async_session()` 创建的 `BuildSession`，参见[构建期间取消后继续观察清理](doc/user_guide.zh_CN.md#构建期间取消后继续观察清理)。

应用只提供共享实例，不提供原型或请求作用域、热更新、未托管组件的自动关闭、循环代理或动态库发现。关闭句柄或 owner 被丢弃时只会尽力请求 abort，不会代替应用等待；`ShutdownReport::incomplete()` 表示仍有组件未确认终止。组件构造不依赖运行时反射；`qubit-spi` 负责 provider 的选择和回退，其注册表或选中服务可作为普通组件注册。

## 延伸阅读

从[中文用户手册](doc/user_guide.zh_CN.md)了解安装、构建、诊断和关闭流程，也可阅读[English user guide](doc/user_guide.md)。生命周期和资源适配的详细约定见[生命周期指南](doc/lifecycle.zh_CN.md)、[托管资源适配指南](doc/managed-adapters.zh_CN.md)及其[英文版](doc/managed-adapters.md)。在源码目录运行 `cargo doc --no-deps --open` 查看公开 API 文档；其他可运行示例位于[`examples`](examples/)。

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

本项目基于 Apache License 2.0 授权。完整许可证文本请参阅[LICENSE](LICENSE)。

## 贡献

欢迎贡献。请遵循 Rust API 指南，及时更新公共 API 文档与测试。使用 `.infra/bin/align-ci.sh` 按项目规则格式化，再用 `.infra/bin/style-check.sh` 或 `.infra/bin/ci-check.sh` 检查。项目样式工具使用 `nightly-2026-06-05`；普通 `cargo fmt --all --check` 使用包要求的 Rust 1.94 工具链，结果不代表项目格式化门禁，应以项目检查结果为准。

## 作者

**Haixing Hu** - *Qubit Co. Ltd.*

仓库地址：[https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)
