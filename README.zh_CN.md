# rs-ioc

[![Rust CI](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-ioc/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-ioc/coverage-badge.json)](https://qubit-ltd.github.io/rs-ioc/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-ioc.svg?color=blue)](https://crates.io/crates/qubit-ioc)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

`qubit-ioc` 计划作为 Qubit 生态的应用级 IoC 容器，用于在应用启动时组装并共享组件。

## 适用对象

需要在启动阶段组装应用组件及其依赖的 Rust 应用开发者。

## 安装

`qubit-ioc` 尚未发布到 crates.io。发布后，可通过 Cargo 将其加入应用：

```toml
[dependencies]
qubit-ioc = "0.1"
```

## 起步说明

当前仓库只有项目骨架，尚无容器公共 API，因此暂时没有可用的代码示例。

## 计划范围

初步设计预计涵盖显式注册实例和工厂、解析依赖、携带依赖路径的缺失依赖与循环依赖错误、按依赖顺序构造组件，以及共享实例。SPI 注册表或其创建的服务可作为组件使用；后端选择和创建回退仍由 `qubit-spi` 负责。仓库结构为以后增加过程宏和可选的 `inventory` 发现机制留出空间。

## 限制

以上是设计目标，当前尚未实现或发布。公共 API 和容器行为将在设计阶段完成后确定。

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
