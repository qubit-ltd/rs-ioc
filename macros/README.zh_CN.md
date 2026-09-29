# qubit-ioc-macros

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc-macros.svg?color=blue)](https://crates.io/crates/qubit-ioc-macros)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](../LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

本 crate 为 `qubit-ioc` 实现 `Component`、`Service`、`Repository`、`Configuration`、
`ConfigurationProperties` 和 `bean` 过程属性宏。宏生成显式注册定义；运行时容器由
`qubit-ioc` 提供。

应用应依赖 `qubit-ioc` 并启用其默认的 `macros` feature，从同一入口获取属性宏与
运行时 API。宏展开使用公开的 `Definition` / `DefinitionBuilder` 注册 API；
隐藏的配置诊断与生成代码 glue 仍是实现细节。运行时与宏应使用匹配的 `0.3.0` 版本。

## 安装

workspace 使用 Rust 2024，要求 Rust 1.94 或更新版本。若应用与仓库 checkout 相邻，
请依赖运行时 facade：

```toml
[dependencies]
qubit-ioc = { version = "0.3", path = "../rs-ioc", default-features = false, features = ["macros"] }
```

`macros` 支持组件声明和配置组。通过 `#[value]` 或 `#[ConfigurationProperties]`
读取配置时，还需要独立的 `config` feature，并登记配置快照。

## 函数 bean 与配置组

`#[bean] fn default_value() -> DefaultValue` 会生成 `DefaultValueBean`，通过
`builder.install::<DefaultValueBean>()?` 安装。使用
`#[bean(marker = CustomFactory)]` 覆盖名称后，安装 `CustomFactory`。
`#[Configuration]` 模块会为组内 bean 导出 `register_ioc(&mut builder)`。
注册只登记定义，根节点决定构建范围，`context.get::<T>()?` 读取构建结果。
分组本身只需要 `macros`。

[完整函数 bean 示例](../examples/readme_beans.rs) 展示了这三种写法。在仓库根目录运行：

```bash
cargo +1.94.0 run --example readme_beans --no-default-features --features macros --locked
```

异步 bean 需要 `build_async()` 或 `build_all_async()`，由应用提供执行器。
需要显式 stop/wait 的资源应在图验证后的工厂内部创建，再返回 `Managed<T>`。
配置 bounded `WaitPolicy` 并保留构建返回的 `Application`；正常退出时调用
`application.begin_shutdown(ShutdownMode::Graceful)`，再等待句柄的 `wait()`。
失败和取消走 Immediate abort 请求。同步和异步构建失败时都立即返回
`BuildFailure`；通过 `cause()` 查看原始错误，用 `take_cleanup()` 或 `into_parts()`
取出清理句柄，再显式等待其 `wait()`。owner、未移交的 `Managed` 和关闭句柄 Drop 会请求
abort 但不等待；查询 context Drop 不关闭资源。完整步骤与迁移见
[生命周期说明](../doc/lifecycle.zh_CN.md)。

## 延伸阅读

公开 API、支持的声明、配置要求和可运行示例见[项目 README](https://github.com/qubit-ltd/rs-ioc/blob/main/README.zh_CN.md)、
[English user guide](https://github.com/qubit-ltd/rs-ioc/blob/main/doc/user_guide.md)和
[中文用户手册](https://github.com/qubit-ltd/rs-ioc/blob/main/doc/user_guide.zh_CN.md)。
仓库当前包含 `0.3.0` 发布候选，尚未发布到 crates.io。

## 测试

以下 workspace 检查需从仓库根目录执行。若当前目录为 `macros`，请先运行
`cd ..` 切换到上一级：

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
[LICENSE](../LICENSE)。

## 贡献

欢迎贡献。请遵循 Rust API 指南，及时更新公共 API 文档与测试，并在提交
Pull Request 前运行 `./align-ci.sh`格式化代码，运行`./ci-check.sh`对齐CI要求。
两个脚本都应从仓库根目录执行，具体入口见[align-ci.sh](../align-ci.sh)与
[ci-check.sh](../ci-check.sh)。

## 作者

**Haixing Hu** - *Qubit Co. Ltd.*

仓库地址：[https://github.com/qubit-ltd/rs-ioc](https://github.com/qubit-ltd/rs-ioc)
