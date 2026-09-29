# qubit-ioc 当前设计

> 本文描述当前实现（0.2.0 发布候选）的设计与公开契约。英文版见[Current Design](complete-design.md)。历史设计背景见[容器内核草案](design.zh_CN.md)和[注解草案](annotation-design.zh_CN.md)；接入步骤见[中文用户手册](user_guide.zh_CN.md)与[English user guide](user_guide.md)。实现和测试是事实依据，本文不承诺未在公开 API 中提供的能力。

## 1. 目标与边界

`qubit-ioc` 在应用启动阶段显式登记应用级共享组件，解析并验证依赖图，再按依赖顺序调用工厂。成功后发布只读 `ApplicationContext`。组件定义可以通过过程宏生成，也可以手动注册；宏不会自动进入容器，应用或 provider 必须调用 `install::<T>()` 或自己的 `register_ioc(&mut builder)`。

容器负责绑定、依赖解析、构造顺序、诊断路径和显式托管关闭。`qubit-spi` 负责 provider 选择与回退。容器不提供请求作用域、原型作用域、热更新、循环代理、自动发现、运行时反射构造或未托管资源的自动关闭。

## 2. 包与 feature

workspace 包含运行时 `qubit-ioc` 与过程宏 `qubit-ioc-macros`。Edition 为 2024，MSRV 为 Rust 1.94。默认 feature 是 `macros + config`。

| Feature | 能力 |
| --- | --- |
| `macros` | 导出 `Component`、`Service`、`Repository`、`Configuration`、`ConfigurationProperties`、`bean` 等声明宏。 |
| `config` | 集成 `qubit-config`，支持配置快照和配置读取。 |

`macros` 与 `config` 独立启用。`#[value]` 和 `#[ConfigurationProperties]` 需要宏与配置能力；纯手动装配可以使用 `default-features = false`。宏声明不会自行注册；即使启用宏，应用仍须显式安装定义。

## 3. 定义与绑定

**定义**是一次组件或工厂注册；**绑定**由 Rust 类型和可选 ID 唯一标识。ID 区分大小写，以点分段；每段以 ASCII 字母开头，后续可含字母、数字和下划线。一个具体实例可通过显式 `bind = dyn Trait` 暴露 trait 绑定；独立 `impl Trait for Type` 不会自动产生绑定。具体键和其接口别名共享同一底层 `Arc`。

宏支持具名字段结构体和单元结构体，以及同步/异步自由函数工厂。组件字段或 bean 参数显式声明依赖；支持单值、精确 ID、可选和集合请求。对其他数据形状，使用手动工厂。字段和 bean 参数的 `cfg` 激活条件会同步投影到宏生成的依赖请求、构造代码和函数调用实参；嵌套 `cfg_attr(..., cfg(...))` 转换为等价条件。被禁用字段上的不支持注入类型不会拒绝该配置；启用字段时会产生带字段 span 的编译错误。`inject`、`value` 等 helper 属性必须直接写在组件字段或 bean 参数上，放进 `cfg_attr` 会得到明确诊断。配置宏需要 `config` feature 和已登记的配置快照。

应用可以用 `BindingOptions` 指定 ID、primary、order 和 profile。profile 在冲突与依赖验证前筛选；无 profile 定义始终生效。每次 `active_profiles` 都替换之前的集合；空集合激活 `default` 与无 profile 定义。多个同类型候选只有在恰有一个 `primary` 时才能满足未指定 ID 的单值请求；指定 ID 时精确匹配，primary 不参与。alias 的 primary/order 不改变具体类型绑定的选择元数据。

`replace_definition(anchor, callback)` 在临时 builder 中登记一个完整替代定义。替代定义必须恰好一次声明 anchor；成功后原定义的所有绑定（包含 alias）都会被替换，所需 alias 必须由新定义重新声明。callback 失败或声明不满足约束时原 builder 不变。

函数 bean 的默认 marker 名称由函数名转成 PascalCase 后追加 `Bean`；
`#[bean(marker = CustomFactory)]` 可指定名称。`#[Configuration]` 模块导出
`register_ioc(&mut builder)`，只负责登记组内 bean，只需启用 `macros`。
[函数 bean 示例](../examples/readme_beans.rs) 展示安装、根节点与读取；读取配置值
仍需要同时启用两个 feature。

## 4. 注册、验证与构造

生命周期阶段为“注册 → 活跃定义与根闭包 → 图验证 → 同步构建的异步工厂预检 → 构造 → 发布 context → 显式关闭”。推荐的装配顺序是：创建 `ContainerBuilder`，登记配置和外部实例，调用 provider 的显式注册入口或逐个 `install::<D>()`，设置 roots/profile，然后构建。注册阶段不运行用户工厂。

`build()` 从一个或多个 `root` 构造所选定义的依赖闭包；至少需要一个 root。`build_all()` 构造所有生效定义，并允许空图。异步对应方法为 `build_async()` 与 `build_all_async()`。若选中图含异步工厂，同步 build 会在运行任何工厂前返回 `AsyncRequired`。异步构建由应用提供执行器，且可以混合同步和异步工厂。

图验证先解析请求候选，再检查缺失、歧义、重复 primary 和环路；同步构建还会预检整个选中图是否包含异步工厂。图验证与适用的同步预检全部通过后，才会运行工厂或 alias projector。可选依赖无候选时解析为 `None`，集合依赖无候选时为空；命中的依赖仍参与图验证。构造顺序保证依赖先于消费者。独立定义按注册和依赖声明顺序稳定排序；构建串行执行。构建成功才发布 context，失败不暴露部分容器。

`BuildContext` 只允许工厂访问其声明并由图解析出的依赖。未声明访问返回 `BuildAccessError`。`ApplicationContext` 查询已构造实例，不会重跑工厂。发布时建立不可变的精确键索引和按类型索引；每个类型索引按 binding order、ID、来源位置和注册位置排序一次。单值与 ID 查询直接遍历该索引，集合查询复用其顺序。`BuildContext::get_all` 和 `ApplicationContext::get_all` 都按 `order`、ID、来源位置升序排列，完全相同的项保留注册顺序。错误候选和可用绑定保留注册顺序。

## 5. 配置与上下游边界

构建后的上下文支持通过 `Arc` 并发执行只读查询。关闭仍由唯一所有者发起：释放共享句柄后用 `Arc::try_unwrap` 取回上下文，再调用 `begin_shutdown(self)`。


启用 `config` 后，`with_config(config)` 将配置快照放入 builder；第二个活跃 `Config` 同键冲突在构建时作为 `BuildError::DuplicateBinding` 返回。`#[value]` 读取配置值，`#[ConfigurationProperties]` 读取结构化子树。缺少快照或反序列化失败在验证或构造阶段返回错误，并保留原始配置错误 source 与组件/字段路径。

provider crate 应导出显式 `register_ioc(&mut builder)`，由应用决定纳入哪些 provider。跨 crate fixture `tests/fixtures/ioc_cross_crate/` 验证 provider 注册、trait alias、profile、配置错误来源及无默认 feature 手动装配；它是契约测试，不代表生产应用采用。

`qubit-spi` 可与 IoC 并用：SPI 在服务族内部选择实现，IoC 负责更大范围的应用依赖与启动顺序。IoC 本身不依赖 SPI，也不隐式发现其他 crate 的 provider。

`#[value]` 与结构化反序列化都不执行插值；结构化反序列化默认拒绝未知字段。应用需要插值时，可在手工工厂中调用 `Config::get_interpolated`。

## 6. 错误与生命周期

错误按阶段表达：`RegistrationError` 表示 ID、profile、重复依赖声明等登记问题；`BuildError` 表示 roots、候选、重复活跃绑定、图、异步要求、配置或工厂问题；`ResolveError` 表示 context 查询问题；`ShutdownError` 聚合资源关闭失败。工厂和配置错误保留 source 链，构建错误尽可能包含完整依赖路径和定义来源。多个 root 保留声明顺序；cycle 诊断包含 root 前缀和实际环路后缀，定义成员补全也保留对应来源关系。

托管资源只能由托管工厂在依赖图验证通过后创建，再通过 `Managed<T>` 注册 stop 和可选的异步 wait。已有外部资源应作为普通实例注册，并由应用自行负责关闭。同步托管工厂可在同步或异步构建中执行，后续工厂失败时已成功移交的资源会参与回滚。应用显式调用 `context.begin_shutdown()`，再对返回句柄调用 `wait().await`。关闭会先按逆构建顺序执行全部 stop，再按该顺序执行 wait；stop 返回错误或 unwind panic 会记录为 `ShutdownFailure` 并继续后续动作。wait 回调创建和 wait future 轮询中的 unwind panic 也会记录为 `ShutdownPhase::Wait` 失败，并继续等待其他组件。`panic = "abort"` 和清理 future 析构时的 panic 无法由此机制捕获。取消 wait 后保留句柄并再次调用 `wait()`，会继续同一个 future。

`Managed<T>` 和 `ShutdownHandle` 使用类型级 `must_use` 提示。托管值应返回给容器；直接丢弃未移交的 `Managed` 不会调用清理。关闭句柄应等待，若有意放弃等待，可显式 `drop(context.begin_shutdown())`，此时 stop 已经执行。

同步构建失败会 stop 已成功返回 `Managed` 的资源但不能 wait；异步构建失败会 stop、wait，并将清理错误与原构建错误一起返回。取消异步构建会按逆构建顺序对已成功返回 `Managed` 的资源各执行一次 stop，但不 wait。工厂在返回 `Managed<T>` 之前已产生的副作用由工厂负责回收。普通 context drop 不会自动关闭资源，外部 `Arc` 克隆可以在关闭后继续持有对象。

## 7. 当前验收依据

当前契约由 `tests/`、宏 crate 测试和跨 crate fixture 验证，重点包括：显式安装和未安装 root 诊断；候选、ID、profile、替换及完整路径；同步/异步构造和取消；trait alias 的共享身份；配置 source 保留；托管 stop/wait 顺序、错误聚合和取消恢复；独立 feature 组合。公共 API 的精确签名以生成的 Rust API 文档为准。完整 worker 启停示例见[生命周期说明](lifecycle.zh_CN.md)与[可运行程序](../examples/app_lifecycle.rs)。


下游验证保留两套独立 manifest/lock：`tests/fixtures/application_consumer/` 固定历史
依赖快照，`tests/fixtures/application_consumer_current/` 使用经审查更新的当前快照。
两条 CI lane 均运行 `check --locked`、`test --locked` 和 `run --locked`，并记录
实际 checkout SHA、Rust 版本和 lock 的 SHA256。当前 lane 应固定包含最新生命周期
夹具的已接受提交；历史 lane 只证明其所选旧源码的契约，不代表包含当前新增测试。
升级快照时应一起更新依赖约束、SHA 和 lock，不使用浮动分支自动更新。
