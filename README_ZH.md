# MT-Engine

[English](README.md) | [中文](README_ZH.md)

**MT-Engine** 是一款基于 Rust 语言实现的极致性能、零堆分配（Zero-Allocation）的高频撮合引擎内核。它专为对时延极其敏感的交易场景设计，原生支持 SBE 编解码。

---

## ⚡ 性能报告与双引擎架构 (Performance & Dual Engine Architecture)

MT-Engine 创新性地提供了两种订单簿后端，允许系统在运行时根据交易品种的流动性特征进行切换。两者均实现了零分配并充分榨取了 CPU 缓存效能：

### 1. 引擎性能矩阵 (Performance Benchmarks)
在 **50,000 单高度饱和稳态** 与 **严格 OrderID 单调递增** 环境下，使用 `MixedWorkload`（涵盖限价单、冰山单、止损单、只做 Maker）进行的端到端（提交 + 成交/撤单）测试结果：

| 后端配置与特性 | 快照功能 (Snapshot) | 平均延迟 (ns/op) | 技术亮点 |
| :--- | :---: | :---: | :--- |
| **`DenseBackend`** | OFF | **~15.6 ns** | 🚀 **O(1)** 逻辑，零泄露稳态管理 |
| **`SparseBackend`** | OFF | ~30.8 ns | 🧩 内存优化 (O(log N) 树结构) |
| **`SparseBackend`** | **ON** | **~30.8 ns** | 🛡️ **已验证零成本抽象 (Zero-Cost)** |
| **`Dense + Snapshot`**| N/A | 互斥设计 | Dense 持久化应使用 `serde` 原始状态映射 |

*(注：上述数据反映了系统在高压采样下的真实表现。由于引入了 **OrderID $O(1)$ 单调性校验金卫**，热路径通过省去哈希表唯一性检索，性能得到了进一步飞跃。)*

### 2. 双引擎核心区别对比 (Architecture Differences)

| 核心特性 | `DenseBackend` 稠密引擎 🚀 | `SparseBackend` 稀疏引擎 🧩 |
| :--- | :--- | :--- |
| **最佳适用场景** | **BTC/ETH 等主流核心资产**，要求极低时延 | **山寨币、长尾资产等**，交易对多，流动性分散 |
| **底层数据结构** | `L3 Bitset` + 静态数组 + 侵入式双向链表 | `BTreeMap` + `Slab` + `HashMap` |
| **最佳买/卖价搜索** | **O(1)** (利用硬件级位运算 CTZ 寻址) | **O(log N)** (基于红黑树导航) |
| **订单排队与撤单** | **O(1) / O(1)** (侵入式指针，原位摘除) | **O(log N) / O(N)** (基于 VecDeque，需遍历) |
| **内存分配模式** | 初始化时一次性预分配，无运行时扩容开销 | 运行时动态分配 (Slab 和 Map 会按需扩容) |
| **空间占用(Footprint)**| **极高** (受设定的全局 `PriceRange` 和大容量影响) | **极低** (按需分配，严格与当前活跃挂单量成正比) |

---

## 🚀 核心特性

- 🛡️ **SBE Native**: 基于 Simple Binary Encoding，实现二进制报文的快进快出。
- 🩸 **Zero-Allocation**: 撮合热路径（Hot Path，含条件单联动触发）彻底消除任何动态内存分配。
- 🧊 **Cache Optimized**: `OrderData` 采用 128 字节对齐，结合 `_mm_prefetch` 硬件预取与 SoA 抽象打磨，最大化提升 CPU 缓存命中率。
- 🔄 **Advanced Features**: 原生支持冰山单、止损触发（O(1) 连环触发机制）、Post-Only 滑点控制、GTD/IOC/FOK 等高级策略及端到端验证。
- 🛡️ **ID Integrity Guard**: 内置 $O(1)$ 级 ID 单调性校验，强制执行严格递增的 OrderID，从源头防止网关侧状态污染。

---

## 🛡️ 集成要求 (Integration Requirements)

为了达成极低时延并确保系统状态的一致性，MT-Engine 的集成方必须遵循以下约束：

### 1. 严格单调递增的 OrderID
每个交易品种/引擎实例的订单 ID 必须是**严格递增且唯一**的：
- **要求**：`new_id > last_order_id`。在 Dense 模式下，OMS 需保证 ID 在 `[1, capacity]` 范围内（通过复用实现），以支持引擎的极速物理预分配。
- **校验机制**：引擎在入口处执行硬件友好的 $O(1)$ 数值比较。
- **异常处理**：任何重复或回退的 ID 请求将被立即拒绝，并返回 `DuplicateOrderId` 错误。
- **技术收益**：这一约束使引擎无需执行昂贵的哈希表查重，从而将报单时延稳定在 20 纳秒以内。

### 2. SBE 协议标准
所有指令（Command）和回报（Report）均遵循 **Simple Binary Encoding (SBE)** 协议：
- **零分配 (Zero-Allocation)**：报文直接在预分配的缓冲区中进行原位编解码。
- **固定偏移量**：支持高效的字段寻址，无变长解析开销。
- **Schema 定义**：请参考 `/schemas` 目录下的 XML 定义文件来生成对应的客户端 Encoder。

---

---

## 📖 快速上手指南 (User Guide)

### 1. 构造与执行订单
使用 `CommandCodec` 进行无锁、零分配的报文构造。引擎支持两种订单簿后端：
- `SparseBackend`: 基于 BTreeMap，适合交易对较多、长尾资产（价格稀疏）。
- `DenseBackend`: 基于 L3 Bitset 和 预分配数组，适合主流资产（价格密集），提供 O(1) 极速性能。

```rust
use mt_engine_core::prelude::*;
use mt_engine_core::codec::CommandCodec;
use mt_engine_core::book::backend::dense::{DenseBackend, PriceRange};
use mt_engine_core::book::backend::sparse::SparseBackend;

// 1. 初始化引擎与缓冲区
let mut resp_buf = [0u8; 1024];
let mut cmd_buf = [0u8; 1024];

// 选择 A: 使用通用稀疏引擎
// let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);

// 选择 B: 使用高性能稠密引擎 (示例: 价格范围 100~200, tick=1, 容量 1024)
let config = PriceRange { min: Price(100), max: Price(200), tick: Price(1) };
let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
let mut codec = CommandCodec::new(&mut cmd_buf);

// 2. 构造一笔 Post-Only 订单
let mut flags = OrderFlags::new(0);
flags.set_post_only(true);

let cmd = codec.encode_submit_ext(
    0,                      // buffer offset
    OrderId(1001),          // order_id
    UserId(201),            // user_id
    Side::buy,
    OrderType::limit,
    Price(10000),           // price
    Quantity(100),          // quantity
    SequenceNumber(1),      // sequence
    Timestamp(1712460000),  // timestamp
    TimeInForce::gtc,
    flags
);

// 3. 执行撮合
let outcome = engine.execute_submit(&cmd);
match outcome {
    CommandOutcome::Applied(report) => println!("Order Placed: {:?}", report.status),
    CommandOutcome::Rejected(fail) => println!("Rejected: {:?}", fail),
}
```

### 2. 运行性能测试
```bash
# 运行策略专项基准测试
cargo bench -p mt-engine-core --bench matching_engine -- Strategies
```

---

## 🛠️ 功能矩阵 (Feature Matrix)

| 功能 | 状态 | 详情 |
| :--- | :---: | :--- |
| **GTC / IOC / FOK** | ✅ | 完整支持 |
| **GTD / GTH** | ✅ | 毫秒级惰性清理 |
| **Post-Only** | ✅ | 价格交叉拦截 (Lazy Validation) |
| **Iceberg** | ✅ | 自动刷新、FIFO 队尾重排及隐形穿透打穿 |
| **Stop / Stop-Limit** | ✅ | LTP (最新成交价) O(1) 触发与零分配级联触发池 |
| **Self-Trade Prevention** | 🏛️ | 建议在 OMS 层处理 |

---

## 开发者文档

- [📜 交易类型定义 (Transaction Types)](docs/TRANSACTION_TYPES_ZH.md)
- [🏗️ 核心架构设计 (Architecture)](docs/ARCHITECTURE_ZH.md)
- [🔌 SBE 集成指南 (SBE Integration)](docs/SBE_INTEGRATION_GUIDE_ZH.md)

---

## 📜 开源协议 (License)

本项目采用 **Apache License 2.0** 协议开源。详情请参阅项目根目录下的 [LICENSE](LICENSE) 文件。
