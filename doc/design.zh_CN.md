# qubit-ioc 容器内核设计

> 此文为早期草案；完整且优先的实现契约见[完整设计](complete-design.zh_CN.md)。

> 状态：设计，尚未实现。本文定义显式注册与容器构建的底层契约；
> [注解与自动装配设计](annotation-design.zh_CN.md)是面向应用开发者的主要使用方案。
> 示例展示预期用法，不是当前 `src/lib.rs` 已提供的 API。

## 1. 目标与定位

`qubit-ioc` 是应用启动阶段的组件组装容器。应用明确注册组件实例或构造工厂，
容器验证依赖关系，按依赖顺序构造所有组件，然后提供只读、可共享的组件集合。
应用可以在开始处理业务请求前发现缺失依赖、循环依赖和构造失败。

一个典型场景是：应用先注册数据库连接池和配置，再注册依赖连接池的仓储，最后
注册依赖仓储的服务。构建成功后，处理器从容器取得服务的共享句柄；处理请求时
不再调用容器工厂。

容器内核的明确目标：

1. 显式注册已有实例、同步工厂和异步工厂。
2. 以 Rust 类型和可选绑定 ID 组成组件键；同一精确键最多有一个绑定。
3. 工厂显式声明必需依赖；构建前验证所有依赖和环路。
4. 以确定的顺序构造全部组件，每个工厂在一次构建中最多调用一次。
5. 构建成功后按类型取得 `Arc<T>`，供多个消费者共享。
6. 失败返回包含组件类型、依赖路径和原始错误的结构化结果。
7. 不绑定 Tokio 或其他异步执行器。

容器内核不负责请求级作用域、临时对象、运行时增删绑定、动态库加载、
启动/关闭钩子或循环依赖的延迟代理。字段注入由过程宏生成普通工厂实现，
配置文件解析由 `qubit-config` 完成。

## 2. 与 qubit-spi 的边界

`qubit-spi` 负责一个服务族内的候选提供者注册、选择、创建与失败回退。
`qubit-ioc` 负责不同组件之间的依赖关系和共享生命周期。两者是组合关系：

```text
应用配置 ────────┐
                 ▼
             qubit-ioc ─────► 共享的业务组件
                 │                  ▲
                 │ 注入注册表        │ 构造时使用选定的后端
                 ▼                  │
       qubit-spi ProviderRegistry ──┘
```

应用可以直接将 `ProviderRegistry<S>` 注册为组件，也可以在 IoC 工厂内调用它的
`resolve()` / `create_configured()`，再把创建结果注册成另一个组件。IoC 不解释
`ProviderSelection`，不复制 SPI 的候选排序或回退状态机，也不要求 `qubit-spi`
成为 `qubit-ioc` 的直接依赖。两种 crate 能独立使用；互操作发生在应用的构造函数中。

例如，一个 `EventBusRegistry` 可在应用配置阶段注册若干实现并封存，然后作为
IoC 实例注册。负责创建 `EventBus` 的工厂声明它依赖该注册表和 `EventBusConfig`。
如果 SPI 创建失败，工厂把原错误作为 source 返回，IoC 在其外层补充组件依赖路径。
IoC 不在 SPI 创建失败后重新选择 provider，因为那会与 SPI 的回退策略重复。

## 3. 核心概念与公共 API 草案

| 概念 | 职责 |
| --- | --- |
| `ContainerBuilder` | 收集绑定并执行构建；构建后被消耗。 |
| `Dependency` | 显式声明工厂需要的组件类型。 |
| `BuildContext` | 在工厂内读取已构造且已声明的依赖。 |
| `ApplicationContext` | 构建成功后的只读组件集合，可在应用中共享。 |
| `RegistrationError` | 重复绑定等注册阶段错误。 |
| `BuildError` | 缺失依赖、环路、异步构建要求或工厂失败。 |

预期的主要操作如下；最终签名以实现时的 Rust 类型检查为准，但行为约束以本文
为准：

```rust,ignore
let mut builder = ContainerBuilder::new();

builder.register_instance::<AppConfig>(Arc::new(config))?;
builder.register_instance::<DbPool>(Arc::new(pool))?;

builder.register_factory::<UserRepository>(
    [Dependency::of::<DbPool>()],
    |context| {
        let pool = context.get::<DbPool>()?;
        Ok(Arc::new(UserRepository::new(pool)))
    },
)?;

builder.register_factory::<UserService>(
    [Dependency::of::<UserRepository>(), Dependency::of::<AppConfig>()],
    |context| {
        let repository = context.get::<UserRepository>()?;
        let config = context.get::<AppConfig>()?;
        Ok(Arc::new(UserService::new(repository, config)))
    },
)?;

builder.root::<Application>();
let context = builder.build()?;
let service: Arc<UserService> = context.get::<UserService>()?;
```

`register_instance::<T>` 接受 `Arc<T>`，`register_factory::<T>` 的工厂返回
`Result<Arc<T>, BoxError>`，其中 `BoxError` 表示可跨线程传递的原始错误。
`T: ?Sized + Send + Sync + 'static`，因此可以显式注册 `Arc<dyn Trait>`，并以
`get::<dyn Trait>()` 获取同一个 trait 对象句柄。容器不尝试自动把具体类型转换成
某个 trait：应用必须明确选择所绑定的接口类型。

`ApplicationContext::get::<T>()` 返回 `Result<Arc<T>, ResolveError>`；
`get_by_id::<T>(id)` 请求精确绑定 ID。`try_get::<T>()` 返回
`Result<Option<Arc<T>>, ResolveError>`：缺失时为 `None`，歧义仍返回错误。
`get_all::<T>()` 返回该类型的全部共享绑定，顺序由显式排序值、绑定 ID 和来源
决定。对应的 `BuildContext` 提供相同的单值、可选值和集合查询，但只可查询
工厂已声明的请求。
容器查询仅查找已构造结果，不触发工厂。

### 3.1 异步工厂

异步工厂使用拥有所有权的 `BuildContext` 和可发送的 boxed future，使 future
可以在 `.await` 期间安全访问已构造的依赖，同时不借用 builder 的内部状态：

```rust,ignore
builder.register_async_factory::<DbPool>(
    [Dependency::of::<DbConfig>()],
    |context| Box::pin(async move {
        let config = context.get::<DbConfig>()?;
        let pool = DbPool::connect(&config.url).await?;
        Ok(Arc::new(pool))
    }),
)?;

let context = builder.build_all_async().await?;
```

`build_async()` 同时接受同步和异步工厂；`build()` 只接受实例与同步工厂。如果
builder 含异步工厂，`build()` 在图验证完成后、调用任何工厂之前返回
`BuildError::AsyncRequired`。因此缺失依赖或环路优先报告。
两个构建方法都消耗 builder。异步构建不自行创建运行时，由应用提供执行器。

## 4. 注册与身份

一个绑定的身份是 `(TypeId::of::<T>(), Option<BindingId>)`。错误与日志同时保留
`type_name::<T>()` 供人阅读；绑定 ID 在注册时校验，不跨不同 Rust 类型
共享命名空间。相同精确键重复注册返回 `RegistrationError::DuplicateBinding`，
不会覆盖先前绑定。未指定 ID 的查询在只有一个候选时选它；有多个候选且恰好一个标为
`primary` 时选主候选；其余多候选情形返回 `AmbiguousBinding`。按 ID 查询只按 ID
匹配。多个同类型的 `primary` 在构建前报错。容器不根据注册先后静默挑选实现。

`BindingId` 的校验规则与 `rs-model-metadata` 的 `#[Entity(id = "...")]` 一致：
非空点分段字符串，每段匹配 `[A-Za-z][A-Za-z0-9_]*`。例如
`example.repository.postgres` 有效，`example..postgres`、`1example.repo` 和
`example.repo-name` 无效。ID 区分大小写，不做规范化，也不从 Rust 类型名
或注册来源自动生成。`None` 是无 ID 绑定；`Some(id)` 必须是有效 ID。
宏的字面量在编译期校验，显式注册的 ID 在注册阶段校验；错误携带原值与来源。
相同 ID 可分别用于不同 Rust 类型，但同一类型中不能重复注册相同 ID。

具体组件与 `dyn Trait` 接口绑定是两个键；宏可以产生从接口键指向具体组件的
别名绑定，让两种查询拿到同一底层 `Arc`。接口别名参与依赖图，绑定 ID 和 `primary`
附着在接口绑定上。多个 SPI 后端之间的运行时选择继续交给 `qubit-spi`。

`register_instance` 表示对象已经由应用创建，不调用工厂。一个实例也可能持有
容器管理之外的资源；容器只管理它收到的 `Arc` 句柄，不声称拥有资源的全部生命周期。
底层注册 API 还需提供带 ID 注册、`primary` 与排序值的显式元数据入口，供宏生成
代码和手写组装共用；未指定这些元数据时使用无 ID、非主候选和排序值 `0`。

工厂依赖用 `Dependency::of::<T>()`、`Dependency::with_id::<T>(id)`、
`Dependency::optional::<T>()` 或 `Dependency::all::<T>()` 声明。
每条请求记录目标类型、可选绑定 ID 和单值／可选／集合基数。
依赖列表按声明顺序保存；重复列出同一请求键在注册时拒绝，以免图和诊断产生
歧义。`BuildContext::get::<T>()` 和 `get_by_id::<T>()` 只能访问当前工厂声明过的
依赖，访问未声明请求返回 `BuildAccessError::UndeclaredDependency`。
即使该类型恰好已构造，也不能绕过依赖图。这保证排序和缺失依赖诊断与工厂实际
可用的对象一致。

## 5. 构建算法

构建分为验证、排序、执行三个阶段：

1. **验证**：先将每条依赖请求按类型、绑定 ID 和主候选规则解析成精确绑定键，
   检查必需依赖的缺失、歧义与多个主候选；没有候选的可选／集合请求不产生
   依赖边。随后用 DFS 的访问状态检测环路。
   错误返回确定的依赖路径，例如 `HttpServer → UserService → UserRepository → DbPool`；
   环路路径的首尾为同一组件。
2. **排序**：生成拓扑顺序。依赖始终排在消费者前面；互不依赖的组件按注册顺序
   构造，同一工厂的依赖按声明顺序遍历。注册顺序保存在 builder 中，不能依靠
   `HashMap` 的遍历顺序。
3. **执行**：实例绑定直接进入已构造集合；同步工厂调用一次；异步工厂顺序等待
   完成。每个工厂得到的 `BuildContext` 只含它声明的依赖句柄。全部成功后才返回
   `ApplicationContext`。

第一版按拓扑顺序**串行构造**，即使多个组件互不依赖，也不并行运行工厂。
这使副作用顺序、错误归属与取消边界可预测。以后若确有启动耗时需求，可在不改变
依赖语义的前提下单独设计并行构建。

验证必须在第一个工厂执行前完成；缺失依赖或环路不能导致一半工厂已经执行。
工厂运行中失败时，不返回部分构建的容器；已构造的 `Arc` 在构建状态被释放时
按正常引用计数规则释放。容器不会回滚工厂已经完成的外部副作用，也不会取消
工厂自行启动的独立任务。

## 6. 存储、共享与生命周期

内部可以用 `(TypeId, Option<BindingId>)` 索引类型擦除的 `Arc<T>` 句柄。擦除层必须允许按注册时的
同一类型还原 `Arc<T>`，包括 `T = dyn Trait`。实现时应区分“用于容器内部类型
擦除的句柄”和“返回给用户的 `Arc<T>`”，避免每次查询重新构造组件或复制 `T`。

`ApplicationContext` 构建后不可变，`get()` 只克隆 `Arc`。同一精确绑定键的多次查询指向同一个
组件实例；组件内部的可变状态由该组件自己选择锁、原子类型或消息传递实现。
容器不提供 `get_mut`，也不把可变借用跨线程传播。

容器被丢弃时释放其持有的组件句柄；调用方保存的 `Arc` 可以让组件继续存活。
因此第一版不承诺确定的资源关闭顺序，也不提供自动关闭回调。需要主动关闭的
资源由应用在丢弃容器前显式调用领域 API。

`build_async()` 的 future 在等待工厂时被取消，后续组件不会继续构造，局部构建
状态随 future 丢弃；已经发生的外部副作用不自动撤销。工厂 panic 按 Rust 默认
机制传播，不被包装成普通 `BuildError`。同步与异步工厂都遵守这一规则。

## 7. 错误模型

建议使用可匹配的错误变体，而不以 panic 表达正常的图错误：

| 阶段 | 错误 | 必须携带的信息 |
| --- | --- | --- |
| 注册 | `DuplicateBinding` | 冲突类型与原绑定的注册位置或顺序。 |
| 注册 | `DuplicateDependency` | 工厂类型与重复的依赖类型。 |
| 验证 | `MissingDependency` | 从起点到缺失类型的完整路径。 |
| 验证 | `AmbiguousBinding` | 请求类型、可用绑定 ID 和主候选状态。 |
| 验证 | `MultiplePrimaryBindings` | 请求类型和所有冲突的主候选来源。 |
| 验证 | `DependencyCycle` | 首尾相同的循环路径。 |
| 验证 | `AsyncRequired` | 至少一个异步绑定的类型。 |
| 构造 | `FactoryFailed` | 失败组件、到该组件的路径、原始 source。 |
| 工厂读取 | `UndeclaredDependency` | 当前工厂与请求的依赖类型。 |
| 查询 | `MissingComponent` | 请求的组件类型。 |

类型路径中的每个节点应有稳定的类型标识和可读类型名；诊断可增加注册位置，
但不把源代码位置作为依赖身份。若一个图同时存在多个问题，按注册顺序及依赖声明
顺序返回第一个问题，从而保证相同输入得到相同诊断。工厂错误保留 source 链，
不把领域错误转成无类型字符串。

## 8. 与 qubit-reflect 的关系

本节参考本地 `rs-reflect` 仓库的 `6a30402` 版本。Qubit 已有的
[`qubit-reflect`](https://github.com/qubit-ltd/rs-reflect) 提供
`#[derive(Reflect)]`、`TypeDescriptor::of::<T>()`、字段与方法描述符、受检动态
构造，以及通过链接片段或显式快照建立的反射注册表。它保存的是**类型和操作的
元数据**，不是应用组件实例。反射能力有显式的本地与线程安全边界；仅有描述符
不能证明一个值满足 IoC 所要求的 `Send + Sync`。

这使反射库适合提供**可选的元数据与工具能力**：

1. 如果组件类型实现 `Reflect`，应用可显式把其 `TypeDescriptor` 关联到 IoC 绑定，
   用于更丰富的类型、字段和来源诊断。未实现 `Reflect` 的普通类型和 trait 对象
   仍可注册，回退到 `type_name::<T>()`。
2. 开发工具可以把 IoC 的依赖图与反射描述符关联，用于展示组件结构和依赖关系。
   反射注册表的快照应由调用方显式传入；IoC 构建不隐式调用全局
   `ReflectRegistry::initialize()`。
3. 在独立的过程宏 crate 中提供 `#[Component]` 等宏。宏在编译期
   从标注的字段或工厂参数生成 `Dependency::of::<T>()` 和普通 builder 注册调用；
   宏生成的绑定仍走第 4～7 节的同一套验证与构造逻辑。若类型也派生 `Reflect`，
   两套元数据可以通过精确的类型身份关联，但宏不能假定反射已自动注册组件。

核心与宏都不要求组件实现 `Reflect`，核心也不直接依赖 `qubit-reflect`。这样当前尚未
对外发布的反射 crate 不会成为 IoC 核心的发布和版本前提。后续确有诊断或工具
需求时，再增加关闭默认值的可选集成特性，并保持依赖方向为
`qubit-ioc → qubit-reflect`。过程宏若需要访问 IoC 运行时，应使用 IoC 自己的
窄接口和版本化生成协议；不复用 `qubit-reflect` 的私有 `codegen_v3` 协议。

反射库的动态 `construct_struct` 要求输入满足其受检构造规则，不能代替任意 Rust
类型的工厂，也不能把具体类型自动转换为 `dyn Trait`。IoC 依赖图需要显式的构造
语义，因此不从反射字段列表猜测依赖，不把“程序中被反射发现的类型”自动注册成
组件。这也避免了为了 IoC 而暴露本应保持私有的字段。

## 9. 静态发现与过程宏的适配边界

面向应用的首版包含可选的 `inventory` 静态发现入口；
应用仍决定哪些 crate 被链接，并由同一 `ContainerBuilder` 路径完成冲突检查、
依赖验证与构造。inventory 的迭代顺序不能成为构造顺序；提交项需有可比较的
稳定来源信息，进入 builder 前先排序。

`#[Component]` 等过程宏只生成显式注册等价的工厂及依赖列表。
宏不能建立另一套容器语义，也不能绕过依赖图校验。宏生成的默认名称只用于
诊断；组件身份继续由类型与可选绑定 ID 决定。宏可与 `qubit-reflect` 的宏同时用于一个类型，
但各自生成独立的契约，不能要求修改反射库的核心注册表。条件绑定、
原型作用域或运行时替换需要独立需求和兼容性设计。

## 10. 建议的实现边界

建议按以下模块组织实现，而不是让单个容器类型兼任全部职责：

| 模块 | 主要职责 |
| --- | --- |
| `key` | `TypeId`、可选绑定 ID 与可读类型名的统一表示。 |
| `binding` | 实例、同步工厂、异步工厂的类型擦除与注册顺序。 |
| `graph` | 缺失依赖检查、环路检测、确定性拓扑排序。 |
| `builder` | 注册 API、构建阶段和工厂调度。 |
| `context` | 限定工厂可读取的已声明依赖。 |
| `application_context` | 不可变的 `ApplicationContext` 及查询 API。 |
| `error` | 注册、图验证、构造和查询错误。 |
| `reflect`（后续可选） | 把显式提供的反射描述符关联到绑定，提供诊断与工具查询。 |

`graph` 不运行用户工厂；`binding` 不决定依赖顺序；`application_context` 不再持有工厂。
这种边界便于单独验证图算法和类型擦除，并使后续宏与静态发现只需调用 builder。

## 11. 验收场景

实现完成后，至少应能通过公开 API 验证下列场景：

1. 注册配置、仓储、服务三级依赖，构建成功；多次 `get` 返回同一 `Arc` 实例。
2. 工厂注册顺序与依赖顺序相反，仍按拓扑顺序构建。
3. 缺失依赖和循环依赖在任何工厂调用前被发现，错误包含可读的完整路径。
4. 重复绑定、重复依赖和访问未声明依赖返回对应的结构化错误。
5. 同步 `build()` 遇到异步工厂时不执行任何工厂；`build_async()` 可以混合构建。
6. 异步工厂失败保留 source；取消异步构建后不执行后续工厂。
7. `Arc<dyn Trait>` 能以 trait 类型注册和查询，而不要求 `Trait: Clone`。
8. `ProviderRegistry<S>` 能作为组件注入；由它创建的服务失败时，IoC 保留 SPI
   的原始错误链且不另行执行候选回退。
9. 未实现 `Reflect` 的普通类型和 trait 对象都可以完成注册、构建和查询；
   引入后续反射集成后，反射注册表的初始化状态也不影响核心构建流程。
10. 同一接口的多个 ID 绑定，唯一主候选和无主候选歧义均按上述规则处理；
    接口别名与具体组件查询共享底层实例。

文档中的 API 示例在实现阶段应转换为可编译的集成测试或 doctest。当前仓库只有
骨架，本文是设计基线，不表示上述场景已经通过运行验证。
