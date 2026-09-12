# Archform POC — 完整规格

> Architecture as Code：拖拽拼装 · 设计时验证 · 契约生成 · AI 可编辑
> 单二进制：Rust(axum) + 内嵌 Web 编辑器。POC 目标：大Joe 八步验收。

## 项目结构

```
archform/
├── Cargo.toml            # crate archform，依赖：axum,tokio,serde,serde_yaml
├── ARCHFORM_SPEC.md      # 本文件
├── examples/openmind.arch.yaml
└── src/
    ├── main.rs           # CLI: archform check|gen|serve [--port 7920]
    ├── model.rs          # 模型定义 + YAML 解析/序列化
    ├── validate.rs       # L1 校验：六契约 + 图规则
    ├── generate.rs       # 生成器：openapi / ts types / mocks
    └── web/
        ├── mod.rs        # axum 路由
        └── editor.html   # 单页编辑器（include_str!）
```

## 1. 模型（model.rs）

```rust
pub struct Arch {
    pub version: u32,
    pub policies: HashMap<String, Policy>,        // 命名策略（横切属性定义）
    pub defaults: Defaults,                       // 继承默认
    pub components: Vec<Component>,
    pub edges: Vec<Edge>,
}
pub struct Policy {
    pub ptype: String,        // auth|trace|crypto|slo
    pub scheme: Option<String>,   // jwt-bearer / opentelemetry / aes-gcm
    pub pdp: Option<String>,      // 必须指向存在的组件 id（横切真服务）
}
pub struct Defaults {
    pub edges: Vec<String>,       // 默认注解策略名
    pub nodes: HashMap<String, HashMap<String,String>>, // kind -> {annotation: value}，如 ai-runtime -> {sandbox: container}
}
pub struct Component {
    pub id: String,
    pub kind: String,             // service|gateway|ai-runtime|datastore|bus|app
    pub x: f64, pub y: f64,      // 画布坐标（编辑器维护）
    pub annotations: HashMap<String,String>,   // sandbox: container|seccomp|off …
    pub ports: Vec<Port>,
}
pub struct Port {
    pub id: String,
    pub ptype: String,            // 六契约之一（下）
    pub role: Option<String>,     // server|client（提示用，校验不强制）
    pub spec: Option<String>,     // openapi:paths=/x / asyncapi:topic=t / schema:v@2 / mcp:tools / prompt→out:s.json / sse:frames
}
pub struct Edge {
    pub id: String,
    pub from: String,             // "component.port"
    pub to: String,               // "component.port"
    pub etype: String,            // 六契约之一
    pub spec: Option<String>,
    pub annotations: Vec<String>, // 策略名列表（叠加 defaults.edges）
}
```

**六契约类型**（pty 既是 Port 插口类型也是 Edge 类型）：
```
sync-call   同步调用   #dc2626 红
event       异步事件   #f59e0b 琥珀
data-store  数据存储   #7c3aed 紫
tool-call   工具调用   #0ea5e9 蓝
model-io    模型I/O   #10b981 绿
stream      流式       #ec4899 粉
```
**组件类型默认端口**（积木盒拖出即带）：
- gateway: [api:sync-call(server), events:event, chat:stream(server)]
- service: [invoke:sync-call(server), emitted:event, store:data-store]
- ai-runtime: [invoke:sync-call(server), llm:model-io, tools:tool-call, session:data-store]
- datastore: [vault:data-store(server), admin:sync-call(server)]
- bus: [topic:event(server)]
- app: [backend:sync-call, ui:stream]

## 2. 校验（validate.rs）— L1，输出结构化错误

```rust
pub struct VErr { pub element: String, /* edge id 或 component id */ pub rule: String, pub message: String }
pub fn validate(a: &Arch) -> (bool, usize, usize, Vec<VErr>)  // (all_ok, pass_count, fail_count, errs)
```
规则清单（每条独立报错，element 可定位）：
1. `unknown-ref`：edge 的 from/to 引用不存在的 component.port
2. `dup-id`：组件/边 id 重复
3. `port-mismatch`：edge.etype ≠ 两端 port.ptype（或两端互不相同）
4. `event-consumer`：event 边必须 from=event口 且 spec 有 topic（缺 topic 报错）
5. `sync-acyclic`：sync-call 子图有环 → 报环路径（a.api→b.inv→c.inv→a.api）
6. `auth-propagation`：sync-call/stream 边缺 auth 注解（含 defaults 继承后仍缺）→ "边 X 裸奔"；policy.pdp 指向不存在组件 → "策略 auth-jwt 的 pdp 不存在"
7. `trace-coverage`：任何边缺 trace 注解
8. `sandbox-floor`：ai-runtime 组件 sandbox 注解缺失或为 off（container/seccomp 通过）
9. `self-loop`：from==to

## 3. 生成器（generate.rs）

`pub fn generate(a: &Arch) -> Vec<(String, String)>`（路径, 内容）：
- `out/openapi/<component>.yaml`：每个 sync-call server 口，从 edge.spec 提取 path 生成最小 OpenAPI 3 文档
- `out/types.ts`：Component/Port/Edge 的 TS interface 全集
- `out/mocks.json`：每组件每端口一个 mock 端点定义（method/path/response schema 占位）

## 4. Web 服务（web/mod.rs）

```
GET  /            → editor.html
GET  /api/arch    → { yaml, graph: Arch }        # 读 archform.yaml（serve 启动目录）
PUT  /api/arch    { yaml }  → 解析→{ ok, errors, graph }（解析失败也 200+errors，编辑器要能显示）
POST /api/validate { yaml } → { ok, pass, fail, errors }
POST /api/generate { yaml } → { files: [{path, content}] } 并写 out/
```
serve 时若目录无 archform.yaml，自动复制 examples/openmind.arch.yaml（内嵌 include_str!）作为初始样例。

## 5. 编辑器（web/editor.html）—— 重头戏

**硬性约束：纯 HTML/CSS/JS，零外部依赖（无 CDN），SVG 一律原生 `<text>`（禁 foreignObject）。**

布局（grid）：
```
┌────────────────────────────────────────────────────┐
│ ARCHFORM ● archform.yaml   [✅ 12/12] [生成⚡] [YAML] │ ← topbar；状态点绿/红
│ 透镜: [拓扑][鉴权][观测][安全]                        │
├────────┬──────────────────────────────┬────────────┤
│ 积木盒   │        SVG 画布              │  检查器      │
│ 160px   │        (flex-1)              │  300px      │
├────────┴──────────────────────────────┴────────────┤
│ 验证台（错误列表，点击跳转）              │ 160px
└────────────────────────────────────────────────────┘
```

画布交互（全部实现）：
- 空白拖动=平移；滚轮=缩放（0.3~2.5）；右下角缩放百分比+复位按钮
- 组件卡片：260×120 圆角矩形，标题+kind 徽章；端口=左右两列小圆（颜色=契约色，title 显示类型）；可拖动（写回 x/y）
- 积木盒拖出 → 新组件落画布（默认端口按 §1 表；id 自动编号 svc-1/svc-2…）
- 端口拖线：mousedown 在端口 → 临时虚线跟随鼠标 → **只有 etype 兼容的对端端口放大+光环**（兼容=双方 ptype 相同）→ 松手创建边（etype=端口类型）
- 边：三次贝塞尔，颜色=契约色；中点小标签显示 etype 中文名；**校验失败的边红色+虚线**
- 点击组件/边 → 检查器：可改 id、看端口列表、删组件/删边（确认）
- 透镜模式：非拓扑模式下，边上的注解以小芯片显示（绿色=有该维度注解；红色=缺）；鉴权透镜=只看 auth；观测=trace；安全=sandbox（组件角标）
- 任何变更（拖放/连线/删除/YAML应用）→ PUT /api/arch → 用返回 errors 刷新状态点+验证台+边的红绿

YAML 分屏：topbar [YAML] 按钮切底部抽屉 → textarea（等宽字体）+ [应用] → PUT → 画布按返回 graph 重绘

验证台：错误行 [规则] element message；点击 → 画布平移居中到该 element 并闪烁高亮

OpenMind 样例（examples/openmind.arch.yaml，开箱即载）：gateway / mind-engine(ai-runtime, sandbox:seccomp) / mind-vault(datastore) / collector(service) / bus / auth-service(service，PDP) / otel-collector(service)——含 policies(auth-jwt 指向 auth-service、otel 指向 otel-collector) 与 defaults，**默认全绿（12 边左右全通过）**，供演示"先通"。

## 6. CLI

```
archform check [file]   # 校验，打印 ✅/❌ 列表
archform gen [file]     # 生成到 out/
archform serve [--port 7920] [--dir .]
```

## 7. 测试（≥10 个）

parse_roundtrip / port_mismatch_reported / unknown_ref_reported / sync_cycle_reported_with_path / auth_missing_reported / pdp_missing_reported / sandbox_floor_reported / defaults_inherited_pass / generate_openapi_writes / generate_ts_writes / api_validate_endpoint / sample_openmind_all_green

## 8. 验收（大Joe 八步）

打开即 OpenMind 样例全绿 → 拖积木 → 拉线见卡口感 → 硬接报红 → 验证台跳转 → 生成出文件 → 切 YAML 改一行画布跟着变 → 透镜看鉴权热力图。
