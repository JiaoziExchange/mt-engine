# MT-Engine 架构设计文档

[English](ARCHITECTURE.md) | [中文](ARCHITECTURE_ZH.md)

## 1. 概述

### 1.1 背景与目标

MT-Engine 是一个使用 Rust 实现的高性能、确定性订单撮合引擎库。专为交易系统设计，遵循 LMAX 架构的核心原则，将撮合引擎实现为一个单线程、确定性的内存状态机。

**核心设计目标：**

- **确定性执行**：相同的输入必须产生相同的输出，支持回测、重放和审计。
- **单线程设计**：避免锁竞争和同步开销，实现可预测的低延迟。
- **内存状态**：所有状态均存储在内存中；持久化和网络通信由外部处理。
- **命令-结果模型**：清晰的输入/输出契约，易于集成和测试。

### 1.2 核心术语

| 术语 | 定义 |
|------|------|
| **订单簿 (Order Book)** | 记录所有未执行限价单的数据结构。 |
| **撮合 (Matching)** | 根据价格-时间优先级配对买卖订单的过程。 |
| **Taker** | 发起交易的主动方，消耗市场流动性。 |
| **Maker** | 等待成交的被动方，提供市场流动性。 |
| **价格档位 (Price Level)** | 同一价格下所有订单的集合。 |
| **时间优先级** | 在同一价格档位内，先提交的订单优先执行。 |
| **SBE (Simple Binary Encoding)** | 用于消息序列化的高性能二进制编码标准。 |
| **确定性** | 给定相同的初始状态和命令序列，保证得到相同的结果。 |

### 1.3 项目阶段

| 阶段 | 状态 | 描述 |
|------|------|------|
| **阶段 1：SBE 协议层** | ✅ 完成 | 支持 OrderSubmit/Cancel/Amend 以及 ExecutionReport/PublicTrade/DepthUpdate 的二进制消息编解码。 |
| **阶段 2：核心撮合引擎** | ✅ 完成 | 实现 LTP 触发的止损/止盈、冰山单重入队及端到端策略。 |
| **阶段 3：性能优化 (Sparse)** | ✅ 完成 | 全链路零分配、SBE unwrap_unchecked、硬件预取、缓存行 ABI 对齐。 |
| **阶段 4：Dense 引擎与可扩展性** | ✅ 完成 | 基于 Bitset 的无树后端，对高流动性资产实现 O(1) 撮合。 |
| **阶段 5：解耦与模块化** | ✅ 完成 | 通过 `OrderEventListener` 解耦 SBE 编码，抽离 `ConditionOrderManager`。 |

### 1.4 设计原则

1. **不可变性优先**：倾向于使用不可变数据结构以减少副作用。
2. **显式优于隐式**：API 设计强调清晰度而非简洁性。
3. **组合优于继承**：使用组合构建复杂类型。
4. **错误即值**：使用 `Result` 类型进行错误处理，而非异常。
5. **零成本抽象**：抽象（如泛型、Trait）不应引入运行时开销。
6. **SBE 兼容性**：暴露的结构体采用固定大小设计，便于 SBE 解析器处理。
7. **单一职责**：将业务逻辑与 I/O 及序列化协议解耦。

---

## 2. 项目结构

### 2.1 代码组织

```
mt-engine/
├── mt-engine-core/         # 核心引擎
│   └── src/
│       ├── engine/
│       │   ├── mod.rs              # 主引擎状态机
│       │   ├── events.rs           # OrderEventListener Trait 定义
│       │   ├── sbe_listener.rs     # SBE 二进制编码器实现
│       │   └── condition.rs        # 条件单管理器 (止损/止盈逻辑)
│       ├── book/                   # 订单簿后端 (Sparse/Dense)
│       ├── outcome/                # 执行结果与 SBE 解码
│       └── types/                  # 核心领域类型 (价格、数量等)
├── mt-engine-sbe/          # SBE 编解码层
├── schemas/                 # SBE XML Schema 定义
└── docs/                    # 文档
```

---

## 3. 核心引擎架构

### 3.1 总体架构

```mermaid
flowchart TB
    subgraph External["外部系统 (用户代码)"]
        Cmd[命令]
    end

    subgraph Core["MT-Engine 核心"]
        Engine[Engine < B, L >]
        Cond[ConditionOrderManager]
    end

    subgraph Listeners["事件处理器"]
        Trait[OrderEventListener Trait]
        Sbe[SbeEncoderListener]
        Mock[MockListener / ()]
    end

    subgraph Book["订单簿组件"]
        Backend[OrderBookBackend Trait]
        Bids[买盘]
        Asks[卖盘]
    end

    Cmd -->|执行| Engine
    Engine <-->|委派| Cond
    Engine -->|分发事件| Trait
    Trait <|-- Sbe
    Trait <|-- Mock
    Engine <-->|Trait 约束| Backend
    Backend --> Bids
    Backend --> Asks
```

### 3.2 解耦事件处理 (模式 A)

为了在提升代码质量的同时保持 sub-100ns 的延迟，MT-Engine 采用 **泛型静态分发** 将撮合循环与 SBE 序列化协议解耦。

- **`OrderEventListener`**：定义了成交、订单接收和撤单回调的 Trait。
- **泛型 `Engine<B, L>`**：引擎对后端 `B` 和监听器 `L` 均实现泛型。
- **零开销**：Rust 编译器会对监听器方法进行单态化和内联，与直接写入缓冲区的性能完全一致。

### 3.3 模块化状态管理

- **`ConditionOrderManager`**：从 `Engine`“上帝对象”中抽离。使用侵入式双向链表和 BTreeMap 管理止损和止盈订单，实现 O(1) 触发。
- **内存安全**：使用 `Slab` 池确保稳定的内存布局和缓存友好的预取。

### 3.4 双引擎后端抽象

针对不同资产的流动性特征，MT-Engine 实现了双后端策略：

| 特性 | `DenseBackend` (高频) 🚀 | `SparseBackend` (通用) 🧩 |
|------|---------------|-----------------|
| **价格查找** | L3 Bitset | BTreeMap |
| **订单映射** | 数组 + 空闲列表 | Slab + HashMap |
| **队列结构**| 侵入式双向链表 | VecDeque / 侵入式触发器 |
| **内存分配** | 预分配 | 动态 (Slab 池) |
| **延迟预期** | < 30ns | < 100ns (经优化) |
| **适用场景** | 主流资产 (BTC/ETH) | 山寨币/NFT/长尾资产 |

---

## 4. 硬件预取与缓存行对齐

### 4.1 OrderData 缓存对齐

```rust
#[repr(C, align(128))]
pub struct OrderData {
    // ========== [热数据：第 0 行 (64 字节)] ==========
    pub remaining_qty: Quantity, 
    pub filled_qty: Quantity,    
    pub price: Price,           
    pub side: Side,             
    // ... 其他热字段 ...

    // ========== [冷数据：第 1 行 (64 字节)] ==========
    pub order_id: OrderId,      
    pub user_id: UserId,        
    // ... 其他冷字段 ...
}
```

### 4.2 硬件预取

MT-Engine 在遍历订单簿时使用显式预取来隐藏内存延迟：
```rust
_mm_prefetch(&self.order_pool[next_idx].data as *const _ as *const i8, _MM_HINT_T0);
```

---

## 5. 时间复杂度

| 操作 | SparseBackend | DenseBackend |
|------|--------|-------|
| 最优买/卖价 | O(log N) | **O(1)** |
| 插入订单 | O(log N) | O(1) |
| 撤销订单 | **O(1)** (侵入式) | **O(1)** |
| 撮合执行 | O(M log N) | O(M) |

*其中 N 为价格档位数量，M 为成交订单数量。*

---

## 6. 健壮性与稳定性

### 6.1 快照增强
- **零拷贝序列化**：利用 `rkyv` 实现高性能内存映射快照。
- **CoW Forking**：使用 `libc::fork` 通过写时复制执行非阻塞快照。

### 6.2 事务一致性
- **回滚机制**：`execute_amend` 确保撮合失败时状态的完整性。
- **触发完整性**：保证 LTP 更新期间止损/止盈订单的级联触发。
