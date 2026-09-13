# SPEC V2：生产级可用性四件套 — Archform

> 目标：工程师打开一眼看懂（分层/中文名/图例）+ 横切可配置（不再手改 YAML）。

## 1. 分层泳道画布（editor.html renderCanvas）

- 画布背景画 4 条横向泳道 + 1 条右侧竖带（在组件图层之下）：
  ```
  应用生态层（app）
  接入层（gateway）
  平台服务层（service / ai-runtime / bus）
  数据层（datastore）
  右侧竖带：横切带（kind 带 cross-cut 标记的组件，见 §4 样例）
  ```
- 泳道：全宽浅灰底 (#fafafa/#f5f5f5 交替) + 左侧竖排/左上角层名（14px 灰字）+ 上下边框虚线
- 组件按 kind 有归属泳道；**拖动时 y 轴吸附到所属泳道中心带**（x 自由）；横切组件吸附右侧竖带（x 吸附，y 自由）
- 画布高度按泳道计算（4×180px + 边距）；fitView 不变

## 2. 中文化 + 图例

- model.rs：Component 加 `pub name: Option<String>`（serde default，display 名；缺省回退 id）
- kind 中文映射（editor.html 常量）：gateway→网关 / service→工程服务 / ai-runtime→AI运行时 / datastore→数据层 / bus→事件总线 / app→应用 / crosscut→横切服务
- 契约中文映射：sync-call→同步调用 / event→异步事件 / data-store→数据存储 / tool-call→工具调用 / model-io→模型I/O / stream→流式
- 组件卡片：标题显示 name（中文）+ 小字 id + 右上角 kind 中文徽章
- 积木盒：每项中文名 + kind 徽章 + title tooltip（"自带端口：invoke(同步) store(数据) emitted(事件)"样式）
- **常驻图例**：画布右上角半透明小卡片，六行（色点+契约中文名），hover 显示一句说明
- 边中点标签继续用契约中文名（现状已是）

## 3. 横切配置 UI（最大件）

- **新端点** `POST /api/serialize {graph}` → { yaml }（web/mod.rs + model.rs 序列化已有，暴露出来）：编辑器结构化改完 graph 后序列化成 yaml 再走既有 PUT /api/arch 保存校验
- **顶栏「策略」按钮** → 右侧抽屉面板（同检查器位置，覆盖显示）：
  - 策略列表（policies）：每行 名称 / 类型(auth·trace·crypto) / scheme / pdp 下拉（可选组件列表）；可新增（+行）/删除
  - 默认继承（defaults.edges）：策略多选 checkbox（选中的默认注到每条边）
  - 「保存」按钮：改 state.graph → POST /api/serialize → PUT /api/arch → 全局刷新（校验结果跟着变）
- **检查器增强**：
  - 选中边：显示"生效注解"区——每条 policy 一个 checkbox（勾=显式注解该边；defaults 继承的显示为半勾/灰字"默认"）；改动走同样 serialize→PUT 流程
  - 选中组件：sandbox 下拉（无/container/seccomp），ai-runtime 类才有
- 透镜模式不变（看配置的效果）

## 4. 样例重做（examples/openmind.arch.yaml）

组件加 name + 调整 kind 归属泳道：
- gateway → name: API网关·五扇门
- mind-engine → name: AI运行时 MindEngine（ai-runtime, sandbox: seccomp）
- mind-vault → name: 知识库 MindVault（datastore）
- collector → name: 采集服务（service）
- bus → name: 事件总线（bus）——归平台服务层泳道
- auth-service → name: 鉴权决策 PDP（kind 改 crosscut，横切带）
- otel-collector → name: 观测收集器（kind 改 crosscut，横切带）
- model.rs kind 校验（若有枚举校验）放开 crosscut；积木盒加"横切服务"积木（端口：invoke 同步 server）
- 样例保持全绿

## 5. 测试（+4 → 20）

- serialize_roundtrip：graph→yaml→parse→相等
- component_name_optional：无 name 时解析 ok 且显示回退
- serialize_policies_defaults：策略与默认继承序列化保真
- sample_crosscut_kind：样例 crosscut 组件解析+校验通过

## 6. 验收

打开 8093：四条泳道+横切带可见、组件带中文名站对层、图例常驻；点「策略」能增删改策略保存后全局绿/红变化；选中边能勾注解；积木盒全中文。测试 20/20 绿。
