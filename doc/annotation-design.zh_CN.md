# qubit-ioc 注解与自动装配设计

> 此文为早期草案；完整且优先的实现契约见[完整设计](complete-design.zh_CN.md)。本文提及的反射可选集成与 `reflect` feature 尚未实现；当前 Cargo feature 不包含 `reflect`。

> 当前运行时要求应用显式调用 `install::<D>()` 或提供者 crate 的 `register_ioc(&mut builder)`；宏不再提交链接器级全局注册项。

> 状态：拟实现的应用层设计，当前仓库尚未提供这些宏和 API。
> [容器内核设计](design.zh_CN.md)定义显式注册、依赖图和共享实例语义；本文定义
> 面向应用开发者的主要入口。示例是目标语法，实施时须以编译测试固定最终签名。

## 1. 使用目标

应用开发者可以在组件和工厂声明处写依赖，用少量启动代码得到完整的应用上下文：

```rust,ignore
use std::sync::Arc;
use qubit_ioc::{ApplicationContext, Component, Service};

trait UserRepository: Send + Sync {
    fn find_name(&self, id: u64) -> Option<String>;
}

#[Component(bind = dyn UserRepository, id = "example.repository.memory", primary)]
struct MemoryUserRepository;

impl UserRepository for MemoryUserRepository {
    fn find_name(&self, id: u64) -> Option<String> { Some(format!("user-{id}")) }
}

#[Service]
struct UserService {
    repository: Arc<dyn UserRepository>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ApplicationContext::builder().discover()?;
    builder.root::<UserService>();
    let context = builder.build()?;
    let service = context.get::<UserService>()?;
    assert_eq!(service.repository.find_name(7).as_deref(), Some("user-7"));
    Ok(())
}
```

`#[Component]` 和 `#[Service]` 在声明处生成注册元数据和类型安全的构造代码；
`discover()` 从最终二进制里已链接的元数据导入绑定；`build()` 验证完整依赖图，
构造所有单例，然后发布只读 `ApplicationContext`。应用仍可使用底层 builder
显式注册实例或工厂，并与宏声明的组件混用。

这种体验借鉴 [summer-rs 的组件工厂宏](https://summer-rs.github.io/docs/getting-started/component/)
和 [shaku 的组件／接口声明](https://github.com/AzureMarker/shaku)，也参考了
[dill 的注解、接口和集合注入用法](https://docs.rs/dill/latest/dill/)。
`qubit-ioc` 采用自己的依赖键、错误和构建流程；这些库是交互设计参考，
不是运行时依赖。

## 2. 首版宏与职责

| 声明 | 可用于 | 生成的行为 |
| --- | --- | --- |
| `#[Component]` | 命名字段结构体、单元结构体 | 从字段推导依赖并构造一个共享组件，可声明接口绑定。 |
| `#[Service]`、`#[Repository]` | 同上 | `#[Component]` 的语义别名，仅增加诊断类别。 |
| `#[bean]` | 自由函数 | 从参数推导依赖，把同步或异步函数注册为工厂。 |
| `#[Configuration]` | 内联模块 | 将模块内的 `#[bean]` 集中声明，可向其传递共同 profile；无隐式运行时配置对象。 |
| `#[ConfigurationProperties(prefix = "...")]` | 命名字段结构体 | 使用 `qubit-config` 把指定配置子树解析为组件。 |
| `#[inject(...)]` | 组件字段、bean 参数 | 指定绑定 ID；无参数时可显式标注普通注入。 |
| `#[value("...")]` | 组件字段、bean 参数 | 从 `qubit-config::Config` 读取单个类型化配置值。 |

所有宏属于首版交付范围，放在独立的 `qubit-ioc-macros` 过程宏 crate 中；
`qubit-ioc` 在 `macros` feature 下重导出。`#[Service]` 与 `#[Repository]` 不施加
额外持久化或事务语义。首版每个组件都是应用级共享单例；“bean”表示一个由
工厂创建并存入上下文的组件，不表示每次查询重新调用函数。

命名约定：标注结构体、枚举或 trait 的宏使用 CamelCase；标注字段或函数的宏
使用 snake_case。`#[Configuration]` 虽标注模块，也作为声明级宏使用 CamelCase。
首版没有面向枚举或 trait 的 IoC 宏；该约定同样约束将来的扩展。

### 2.1 结构体组件与字段注入

在 `#[Component]`、`#[Service]` 或 `#[Repository]` 结构体中，未标记的
`Arc<T>` 字段按类型注入。允许的注入形状为：

- `Arc<T>`：必须存在且唯一；若有多个候选，按第 4 节的主候选规则解析。
- `Option<Arc<T>>`：没有候选时为 `None`；有多个且无法唯一选择时仍报歧义。
- `Vec<Arc<T>>`：注入该接口的全部绑定，没有候选时为空。
- `#[inject(id = "...")] Arc<T>`：按绑定 ID 精确选择。
- `#[value("path")] T`：由 `qubit-config` 类型化读取。此字段不作为组件依赖。

`T` 可以是具体类型或满足 `Send + Sync + 'static` 的 trait 对象。宏生成
`Arc<T>` 的查询与结构体字面量构造，不要求组件本身实现 `Clone` 或 `Reflect`。
普通标量字段如果既不是上述注入形状，也没有 `#[value]`，宏给出编译错误，
提示改用 `#[bean]` 工厂。字段保持原有可见性；生成代码位于声明模块内，
不会把私有字段变为公开字段。元组结构体、泛型组件和带生命周期参数的组件
暂不作为首版宏输入；它们仍可通过显式工厂注册。

```rust,ignore
#[Service]
struct SearchService {
    #[inject(id = "example.search.fast")]
    search: Arc<dyn SearchBackend>,
    audits: Vec<Arc<dyn AuditSink>>,
    #[value("search.page_size")]
    page_size: usize,
}
```

### 2.2 工厂函数与配置模块

当构造需要自定义逻辑或异步 I/O 时使用 `#[bean]`。它支持自由函数的
`T`、`Arc<T>`、`Result<T, E>` 和 `Result<Arc<T>, E>` 返回形状，以及相应的
`async fn`；`E` 必须满足 `Error + Send + Sync + 'static`。宏剥离仅供它自身
识别的参数属性，保留原函数供普通调用。返回类型别名或其他无法从语法可靠
识别的普通输出类型别名可通过 `#[bean(type = T)]` 指定目标类型；隐藏了
`Result` 结构的别名需要改写为显式 `Result<T, E>`，或使用底层工厂注册。
无法验证的签名产生定位到函数或参数的编译错误。

```rust,ignore
#[Configuration]
mod data_access {
    use super::*;

    #[bean]
    async fn db_pool(settings: Arc<DbSettings>) -> Result<DbPool, DbError> {
        DbPool::connect(&settings.url).await
    }

    #[bean(bind = dyn UserRepository, id = "example.repository.postgres", primary)]
    fn user_repository(pool: Arc<DbPool>) -> PgUserRepository {
        PgUserRepository::new(pool)
    }
}
```

`#[Configuration]` 只提供声明分组和来源定位；模块中的 `#[bean]` 与模块外的
`#[bean]` 走同一注册流程。它不会模仿 Java 的代理对象，也不会拦截 bean 函数
之间的直接调用：直接调用仍是一次普通 Rust 函数调用。若希望取得容器共享实例，
应把目标组件作为函数参数注入。

组件与 bean 的常用属性统一为 `bind = dyn Trait`、`id = "..."`、`primary`、
`order = 整数` 和 `profile = "..."`。`primary` 与 `order` 作用于生成的接口绑定；
没有 `bind` 时作用于该组件自身的绑定。一个声明需要暴露多个接口时可重复写
`bind`；绑定 ID 在每个接口的类型命名空间内分别校验。模块级 `profile` 只作为
未显式指定 profile 的内层 bean 的默认值。

绑定 ID 的语法与 `rs-model-metadata` 的
[`#[Entity(id = "...")]` 校验规则](../../rs-model-metadata/src/model_id.rs)保持一致：
非空的点分段字符串，每段匹配 `[A-Za-z][A-Za-z0-9_]*`。例如
`example.repository.postgres` 合法；空串、`.example`、`example..cache`、
`example.1cache`、含连字符或非 ASCII 字符的 ID 均非法。宏要求字符串字面量，
并在编译期对声明的 `id` 和 `#[inject(id = "...")]` 报带 span 的错误；手写注册
API 在注册时按同一规则校验。ID 区分大小写，保留原文，不做大小写、分隔符或
空白规范化。省略 `id` 表示无 ID 绑定，不从 Rust 类型名或函数名隐式生成 ID；
来源名称只用于诊断。`#[inject(id = "...")]` 必须与目标绑定声明的 ID 完全一致。

### 2.3 配置注入

`qubit-config` 是配置解析层。应用先显式加载或合并配置来源，再把最终
`Config` 快照交给 builder；IoC 不规定文件名、环境变量前缀或来源覆盖顺序。
`#[ConfigurationProperties]` 生成一个依赖 `Config` 的工厂，调用
`ConfigSerdeExt::deserialize::<T>(prefix)` 构造类型化配置对象；因此该结构体
需要实现 `serde::de::DeserializeOwned + Send + Sync + 'static`。

```rust,ignore
use serde::Deserialize;
use qubit_ioc::ConfigurationProperties;

#[derive(Deserialize)]
#[ConfigurationProperties(prefix = "db")]
struct DbSettings {
    url: String,
    max_connections: u32,
}

let config = load_application_config()?; // 应用选定 qubit-config 的来源和覆盖规则
let context = ApplicationContext::builder()
    .with_config(config)
    .discover()?
    .build_async()
    .await?;
```

`#[value("path")] T` 对应 `Config::get::<T>(path)`；缺失、类型转换、插值或
未知字段错误保留 `qubit-config` 的原始 source，并由 IoC 增加组件和字段路径。
没有 `with_config` 时，配置相关绑定在执行任何工厂前报缺失 `Config` 依赖。
配置只在启动构建时读取一次，已经构造的组件不随原始配置变化而自动刷新。

## 3. 启动与发现

主要入口是 `ApplicationContext::builder()`。`discover()` 收集宏在已链接 crate
中提交的静态注册项，再导入同一个底层 `ContainerBuilder`。无宏场景可完全通过
`register_instance`、`register_factory` 和 `register_async_factory` 构建。

过程宏为每个声明生成两种入口：

1. 一个明确的注册函数或 `ComponentDefinition::register(builder)` 实现，允许
   应用在关闭 inventory 时手动安装组件。
2. 启用 `inventory` 时，一个指向该注册入口的静态提交项，供 `discover()` 收集。

宏生成的提交调用由运行时提供的隐藏 `submit_component!` 宏封装：运行时启用
`inventory` 时它提交静态项，关闭该 feature 时它展开为空。生成代码不能在
消费方 crate 中使用 `#[cfg(feature = "inventory")]` 判断依赖 crate 的 feature，
因为该 `cfg` 检查的是消费方自身的 feature。`#[Configuration]` 不再为子项额外
提交一份注册项，以免与各 `#[bean]` 的提交重复。

应用必须确保含提交项的 crate 实际进入最终链接产物；仅在 `Cargo.toml` 声明依赖
并不保证这一点。跨 crate 组件可集中在应用装配模块中用 `use provider_crate as _;`
固定链接。`discover()` 不是扫描磁盘、模块路径或运行时动态库。

发现项在执行注册函数前按稳定的来源键排序。来源键由包名、模块路径、文件、
行列信息组成，仅用于诊断和确定性顺序，不是组件身份。过滤 profile 后，
宏定义和手写绑定都进入同一冲突检查、依赖解析与拓扑排序流程。重复提交同一
精确绑定键返回错误，不由链接顺序决定胜者。

对测试或应用覆盖，builder 提供显式 `exclude_component::<T>()`（发现前声明，
跳过该组件及其生成的接口别名）和 `replace_binding`（发现后替换一个精确键）。
替换必须显式声明目标键；若影响已生成别名，应单独处理该别名或排除其来源组件。
不提供按注册顺序隐式覆盖。

## 4. 接口绑定、ID 与集合

组件身份由 `(Rust TypeId, 可选绑定 ID)` 组成。`#[Component(bind = dyn Trait)]` 或
`#[bean(bind = dyn Trait)]` 在注册具体组件时额外生成接口别名；别名与具体组件
共享同一底层 `Arc`。编译器通过生成的强制类型转换检查具体类型实现了该 trait。
宏可以重复 `bind` 声明，让一个组件暴露多个接口。声明的 `id` 同时用于具体
组件绑定和每个生成的接口别名；注入 trait 对象时按接口类型和 ID 查找别名。

```rust,ignore
#[Component(bind = dyn Cache, id = "example.cache.local", primary)]
struct LocalCache;

#[Component(bind = dyn Cache, id = "example.cache.remote")]
struct RemoteCache;
```

无名的 `Arc<dyn Cache>` 注入只有一个候选时选它；有多个候选时要求恰好一个
`primary`。`#[inject(id = "example.cache.remote")]` 精确选择该 ID，不受主候选影响。
不存在候选时报缺失，多个候选且无唯一主候选时报歧义；同一接口有多个主候选
在构建前报配置冲突。ID 仅在相同类型的命名空间中比较，不要求跨类型全局唯一。

`Vec<Arc<dyn Cache>>` 注入该接口所有绑定，按显式 `order` 升序、原样 ID
升序、来源键升序排列；无 `order` 时为 `0`，无 ID 绑定排在有 ID 绑定之前。
不得把 inventory 原始迭代顺序
作为集合顺序。`Option<Arc<T>>` 只将“完全缺失”转成 `None`，不能掩盖歧义、
循环依赖或工厂错误。可选和集合依赖在候选解析后转为依赖图边，存在的候选
仍参与环路检测。

首版不支持运行时按 ID 模糊搜索组件，也不根据配置值隐式改写 ID。运行时从多个
后端中选择并在创建失败后回退的需求继续使用 `qubit-spi`。

## 5. 宏展开与容器内核的契约

过程宏实现分为四步，避免把依赖推导散落在生成代码中：

1. **解析**：分别把结构体字段、函数参数、返回形状和属性解析为同一份组件声明
   中间表示；记录每个语法节点的 span 与来源位置。
2. **校验**：拒绝不支持的结构体形状、泛型与生命周期、非法 ID、重复属性、
   `primary`／`bind` 组合错误，以及无法确定的返回形状。此阶段只检查单个声明；
   不假装知道其他 crate 中有哪些组件。
3. **依赖推导**：把 `Arc<T>`、`Option<Arc<T>>`、`Vec<Arc<T>>`、绑定 ID 和
   `#[value]` 分别转换成内核可理解的请求描述；保留字段或参数名供诊断。
4. **生成**：发出保留原声明的强类型构造函数、显式注册入口，以及可选的
   inventory 提交项。生成代码只调用内核公开的注册语义和窄的隐藏协议。

以结构体组件为例，宏概念上生成以下内容：

```rust,ignore
// 概念性展开，不是逐 token 的最终输出
impl ComponentDefinition for UserService {
    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        builder.register_factory::<Self>(
            [Dependency::of::<dyn UserRepository>()],
            |context| {
                let repository = context.get::<dyn UserRepository>()?;
                Ok(Arc::new(Self { repository }))
            },
        )
    }
}

qubit_ioc::__private::submit_component!(
    RegistrationEntry::new(<UserService as ComponentDefinition>::register, source_location!())
);
```

真实生成代码使用 `qubit_ioc::__private::codegen_v1` 中按需导出的窄协议，
以支持依赖重命名和以后版本迁移；不要通配重导出运行时内部模块。宏 crate
只依赖语法解析库，不依赖运行时 crate，避免 Cargo 循环依赖。宏保留用户类型、
函数和文档属性，只移除自身消费的字段或参数属性。

宏按语法生成强类型构造代码，Rust 编译器负责检查 trait 转换、`Send + Sync`、
返回类型和字段可见性。宏不能跨 crate 扫描其他组件，因此缺失绑定、多个实现
和跨声明环路在 `discover()` 后的图验证阶段报告。所有运行期图错误应返回
结构化 `BuildError`；正常的缺失依赖或环路不触发 panic。函数内部显式 panic
仍按 Rust 的 panic 语义传播。

## 6. 构建、生命周期与失败

容器内核先合并手写与宏注册项，按已选 profile 过滤，再解析单值、可选、集合
请求和接口别名。它在调用第一个工厂前检查重复、缺失、歧义、多个主候选和环路。
验证成功后按确定的拓扑顺序**串行**构造全部组件。`build_async()` 同时接受同步
与异步工厂；`build()` 遇到异步工厂时，在图验证之后、任何工厂执行之前返回
`AsyncRequired`。不内置 Tokio，也不在构建过程中隐式创建执行器。

每个绑定在一次成功构建中构造一次。`ApplicationContext::get` 只克隆 `Arc`，
不会重新执行工厂。`Arc<T>`、`Arc<dyn Trait>` 与同一组件的别名共享对象身份。
构建失败不发布部分上下文；已经执行的数据库连接、文件写入等外部副作用不保证
回滚。异步构建被取消时，当前 future 和局部状态按 Rust 规则丢弃，后续工厂
不会开始；独立启动的任务需要应用自己管理。

profile 是启动时的静态过滤条件：`#[Component(profile = "prod")]`、
`#[bean(profile = "test")]` 与 `builder.active_profiles(["prod"])` 配合使用。
未设置时只有 `default` profile 活跃；未标注 profile 的组件在所有 profile
下生效。过滤发生在依赖图验证之前，过滤后缺失的依赖仍是构建错误。条件缺失
注册、运行时刷新和请求级作用域留待后续独立设计。

## 7. 与 qubit-reflect、qubit-spi 的边界

`qubit-reflect` 提供类型、字段和方法元数据，以及受检动态构造；它的
`ReflectRegistry` 保存元数据，不保存组件实例。IoC 宏直接分析自己的 Rust
声明并生成强类型构造代码；它不使用反射字段列表推断依赖，也不要求业务组件
派生 `Reflect`。未来若有明确需求，可单独设计 `TypeDescriptor` 与绑定的关联；
当前没有可选集成层或对应 Cargo feature，也不会隐式初始化全局反射注册表。
这保持 `qubit-reflect` 的线程安全边界和内部生成协议独立。

`qubit-spi` 的注册表可作为组件注入，`#[bean]` 工厂可以调用 SPI 的
`resolve()` 与 `create_configured()`，把选定后端注册为共享组件。IoC 只负责
跨组件的构造顺序和错误上下文；provider 选择、优先级和失败回退仍在 SPI。
两个集成都不要求 IoC 内核在默认 feature 下直接依赖相应 crate。

## 8. 包结构与功能开关

建议在同一仓库中维护两个发布单元：

```text
rs-ioc/
├── Cargo.toml             # qubit-ioc 运行时与 workspace
├── src/                   # 容器内核、发现入口、公开重导出
├── macros/                # qubit-ioc-macros，过程宏 crate
├── doc/                   # 设计与使用说明
└── tests/                 # 跨 crate 与宏诊断场景
```

默认 feature 为 `macros + inventory + config`，让普通应用可直接使用注解、
静态发现和 `qubit-config` 集成。`default-features = false` 保留纯显式组装内核；
反射元数据集成没有纳入当前 feature 集合。`qubit-ioc-macros` 通过运行时的
版本化生成协议输出代码，不调用 `qubit-reflect` 的私有 `codegen_v3`。

当前 `rs-ioc` 仍是单 crate 骨架；增加 workspace、宏 crate、feature 和依赖属于
后续实施工作，不在本文档修改范围内。

## 9. 诊断与验收

宏在编译期报告不支持的字段类型、函数返回形状、非法属性组合、重复字段属性
以及无法证明的接口转换；错误应定位到对应声明。构建期错误至少保留请求类型、
绑定 ID、注册来源和依赖路径；配置与工厂错误保留原始 source 链。

交付前需要用可编译的跨 crate 场景固定以下行为：

1. `#[Component]`／`#[Service]`／`#[Repository]` 在已链接 crate 中自动发现，
   且手写 `ComponentDefinition::register` 与发现结果等价。
2. 字段注入、同步与异步 `#[bean]`、`#[Configuration]` 分组和配置属性注入
   均只构造一次；查询返回同一 `Arc`。
3. 两个接口实现分别按 ID 查询；未指定 ID 的查询遵循唯一主候选规则，歧义返回
   所有候选的来源信息。
4. `Option<Arc<T>>` 和 `Vec<Arc<T>>` 按缺失、歧义和确定性顺序处理。
5. profile 过滤先于图验证；排除后缺失依赖返回清晰路径。
6. 宏声明与显式注册混用时重复键报错；显式覆盖和排除按精确键或组件来源生效。
7. 语法错误为带 span 的编译错误；跨组件环路、缺失、工厂失败为结构化构建错误。
8. 未实现 `Reflect` 的组件仍可自动装配；`qubit-spi` 失败链在 `#[bean]` 外层保留。
9. 关闭默认 feature 时，纯底层显式注册与构建不依赖宏、inventory、配置或反射。

这些是目标契约，当前仓库骨架尚未实现，也未以运行测试验证。
