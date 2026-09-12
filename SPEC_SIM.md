# SPEC 补充：仿真流（L2 可视化）— Archform

> 点「仿真 ▶」→ 光点沿边流过全图；卡在哪一目了然。
> 后端出"轨迹"，前端做动画。POC 级：轨迹级仿真（不真跑 mock HTTP）。

## 1. 后端：POST /api/simulate { yaml }（web/mod.rs 或新 src/simulate.rs）

```rust
pub struct SimStep {
    pub step: String,       // enter | process | hop | stall | done
    pub element: String,    // 组件 id（process）或边 id（hop/stall）
    pub message: Option<String>,
}
// 返回 { ok: bool, entry: String, steps: Vec<SimStep>, hops: usize }
```

轨迹算法：
1. 校验 yaml（复用 validate）。选入口：kind==gateway 且有 sync-call server 口的组件（第一个）；没有 gateway 就选 app 组件的第一个 client 口反向找；都没有 → `{ ok:false, steps:[{step:"stall", element:"-", message:"找不到入口（需要 gateway 或 app 组件）"}] }`
2. 从入口 DFS：`enter gateway.api` → `process gateway` → 对 gateway 每条**出边**（from 含该组件）按序：若该边在 errors 里（element 匹配）→ `stall`（带错误 message），ok=false，**终止**；否则 `hop <edge_id>` → 递归目标组件（`process`），每组件只处理一次（visited），深度上限 14 步
3. 全部走完无 stall → 末尾 `done`（message: "全链路 N 跳逻辑通"），ok=true
4. 环：visited 已防死循环；遇到已访问组件直接 hop 过去不再 process

## 2. 前端：topbar 加「仿真 ▶」按钮（editor.html）

- 点击：POST /api/simulate（用当前 state.yaml）→ 播放动画
- 动画（在现有 svg 上叠一层 overlay `<g>`，不重渲染画布）：
  - 光点 = 半径 7 的圆，品牌红 #f59e0b 发光（filter 或双层圆），初始在入口组件中心
  - process：组件卡片外圈脉冲光环（描边圆扩散渐隐）
  - hop：光点沿边的贝塞尔曲线移动（把 renderCanvas 里 portPos/控制点计算提成可复用函数 edgeGeom(e)；动画 rAF 按弧长参数 t:0→1 插值 ~600ms/跳）
  - stall：光点在断点闪红 3 次 + 该边粗红描边，验证台插入错误行，toast "仿真卡在 <edge>"
  - done：光点淡出，走过的边保持金色描边高亮 5 秒；验证台插入 "仿真完成：入口 X，N 跳全通"
- 播放中：仿真按钮变「仿真中…」禁用；点画布任意处可跳过动画直接显示结果
- 结果同时在验证台显示一行（绿色✅或红色❌带 element）

## 3. 测试（+4）

- simulate_all_green：openmind 样例 → ok=true，steps 首步 enter gateway，末步 done，无 stall
- simulate_stall_on_broken：样例里把一条边 from 改成不存在的口 → ok=false，含 stall 步
- simulate_no_entry：空组件表 → ok=false 带提示
- simulate_cycle_safe：手造 A↔B 环 → 不死循环，hops 有界

## 4. 验收

点「仿真 ▶」→ 看到光点从 gateway 入场 → 沿红边跳到 mind-engine → 紫边进 mind-vault → …→ done 金色高亮；故意接坏一条边再仿真 → 光点卡住闪红 + 验证台报"卡在"。
