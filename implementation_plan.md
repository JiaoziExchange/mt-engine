# 撮合引擎输出协议升级计划 (终极修订版 - 已审阅)

本计划将 `mt-engine` 的输出重构为支持多态、对齐且高性能的 **SBE 事件流**，并包含完整的回归测试要求。

## 1. 核心设计策略
- **私有/公共隔离**: 
    - **OMS 域**: `ExecutionReport` (包含 UserId, LeavesQty, CumQty)。
    - **MD 域**: `PublicTrade` & `DepthUpdate` (仅在 `!dense-node` 时开启)。
- **极致性能**: 
    - **零 Syscall**: 时间戳严格沿用 Ingress 传入值。
    - **8 字节对齐**: 每个 SBE 消息后填充至 8 字节边界。
    - **零成本 Dense 优化**: 使用 `#[cfg(not(feature = "dense-node"))]` 彻底隔离 MD 逻辑。
- **自动化生成**: 使用 `scripts/generate-sbe.sh` 脚本驱动 Docker 进行代码生成。

---

## 2. 核心逻辑实现

### 协议代码库 ([mt-engine-sbe](file:///Users/devine/code/rust/mt-engine/mt-engine-sbe))
1.  修改 `schemas/mt-engine/templates_FixBinary.xml`：定义 `ExecutionReport`, `PublicTrade`, `DepthUpdate`。
2.  执行 `./scripts/generate-sbe.sh` 生成新 Codec。

### 内核驱动 ([mt-engine-core](file:///Users/devine/code/rust/mt-engine/mt-engine-core))
- **`OrderEventListener` 扩展**:
    - `on_accepted`, `on_cancelled`, `on_rejected`, `on_amended`, `on_expired`, `on_trade`, `on_depth_update`。
- **Engine 埋点**:
    - 在各指令热路径插入事件钩子，确保 Dense 模式下 MD 钩子被 `#[cfg]` 剥离。
- **SbeEncoderListener**:
    - 升级以支持多 Template 编码，并实现 8 字节对齐。

---

## 3. 验证与回归计划 (关键)

### 自动化测试
- **Cargo Test**: 运行全量单元测试，确保基本业务逻辑（撮合、撤单、快照）无损。
- **自定义脚本验证**:
    - 执行 `./test_snapshots.sh`: 验证结构变更后快照恢复是否依然一致。
    - 执行 `./test_benches.sh`: 验证 Refactor 后的性能表现。
- **对齐与多步验证**: 构造 Taker 吃掉 10 笔 Maker 的用例，验证回显报文的连续性与 8 字节对齐。

### 性能基准
- **Cargo Bench**: 对比重构前后的 Latency，确保热路径开销控制在纳秒级。

---

## 4. 任务清单
- [ ] 修改 SBE XML 模式
- [ ] 运行脚本生成代码
- [ ] 扩展 Event Trait 接口
- [ ] 更新 Engine 核心逻辑埋点
- [ ] 升级 SBE 监听器
- [ ] 执行单元测试 (cargo test)
- [ ] 执行快照测试 (test_snapshots.sh)
- [ ] 执行性能基准 (cargo bench / test_benches.sh)
