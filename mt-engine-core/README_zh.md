# MT-Engine Core

本项目是 **MT-Engine** 的核心撮合内核，采用单线程、事件驱动的状态机架构。

## 零堆分配架构 (Zero-Allocation)

为了追求极低且确定的延迟，`mt-engine-core` 在撮合路径减少堆分配。

### 缓冲区注入策略
`Engine` 通过生命周期借用用户提供的外部响应缓冲区：
```rust
let mut resp_buf = [0u8; 65536]; // 栈或静态分配

// 选择 A: SparseBackend (适用于长尾资产)
// let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);

// 选择 B: DenseBackend (适用于主流资产，要求 O(1) 延迟)
use mt_engine_core::book::backend::dense::{DenseBackend, PriceRange};
let config = PriceRange { min: Price(1), max: Price(10_000), tick: Price(1) };
let mut engine = Engine::new(DenseBackend::new(config, 100_000), &mut resp_buf);
```
这意味着引擎在生成成交回报（Trades）时，是直接在预定义的内存空间内进行偏移写入，而非动态分配 `Vec`。

## 核心 API

### 1. 指令编解码工厂 (CommandCodec)
屏蔽 SBE 底层字节对齐逻辑，提供语义化报文构造入口：
```rust
let mut cmd_buf = [0u8; 1024];
let mut codec = CommandCodec::new(&mut cmd_buf);

// 极速构造限价单提交报文 (返回一个带生命周期的 Decoder)
let decoder = codec.encode_submit(0, order_id, user_id, Side::buy, price, qty, seq, ts, TimeInForce::gtc);
engine.execute_submit(&decoder);
```

### 2. 安全成交迭代器 (TradeIterator)
所有撮合结果通过类型化迭代器安全读取，彻底杜绝原始字节偏移量计算风险：
```rust
if let CommandOutcome::Applied(report) = outcome {
    for trade in report.trades() {
        println!("Trade: {} @ {}", trade.quantity(), trade.price());
    }
}
```
## 支持的指令与订单类型

MT-Engine 核心通过 `CommandCodec` 与 `Engine` 配合，完整支持以下工业级交易场景：

### 1. 基础订单类型 (Order Types)
- **限价单 (Limit Order)**: 严格遵循价格-时间优先原则。
- **市价单 (Market Order)**: 以 TIF=IOC 模式极速横扫对方盘口，直至全部成交或流动性枯竭。
- **止损单 (Stop / Stop-Limit)**: 由 **LTP (最新成交价)** 驱动。订单初始进入触发池，当 LTP 满足触发价时自动激活进入撮合。

### 2. 生效策略 (Time In Force)
- **GTC (Good Till Cancel)**: 永久有效，直至成交或被撤单。
- **IOC (Immediate Or Cancel)**: 立即成交，未成交部分自动撤单。
- **FOK (Fill Or Kill)**: 必须全部成交，否则整单取消。
- **GTD/GTH (Good Till Date/Hour)**: 结合 SBE 协议中的时间戳实现**延迟失效检查**。

### 3. 高级执行策略 (Advanced Strategies)
- **只做 Maker (Post-Only)**: 采用 **Lazy Validation** 机制，若订单会立即成交则直接拒绝，确保订单始终作为 Maker 进入深度，享受手续费返还。
- **冰山订单 (Iceberg)**: 自动管理 `visible_qty` 和 `peak_size`。当可见部分成交后，系统自动执行 $O(1)$ 重排队并刷新盘口。

---

## 性能白皮书 (Final Benches)

基于 **Apple M4 (Criterion Optimized)** 的实测数据，所有指标均在极高负载下保持纳秒级抖动：

| 撮合场景 | 平均延迟 (Latency) | 吞吐量 (Throughput) |
| :--- | :--- | :--- |
| **标准限价单匹配 (Standard Limit)** | **~31.3 ns** | ~31.9M ops/sec |
| **单档连续撮合 (Single Level)** | **~34.4 ns** | ~29.1M ops/sec |
| **深度横扫 (Sweep 20 Levels)** | **~32.2 ns** | ~31.0M ops/sec |
| **冰山单重排队 (Iceberg Refresh)** | **~31.7 ns** | ~31.5M ops/sec |
| **GTD 延迟检查 (GTD Matching)** | **~28.6 ns** | ~34.9M ops/sec |

| 管理与开销 | 平均延迟 (Latency) | 吞吐量 (Throughput) |
| :--- | :--- | :--- |
| **极速撤单 (Cancel Order)** | **~2.1 ns** | ~476M ops/sec |
| **Post-Only 拦截 (Rejected)** | **~7.6 ns** | ~131M ops/sec |
| **Stop Order 触发检查 (Inactive)** | **~8.6 ns** | ~116M ops/sec |
| **SBE 编解码 (9 Fields)** | **~2.7 ns** | ~370M ops/sec |

> [!TIP]
> 上述数据包含完整的 SBE 编码、内存安全性检查以及订单簿状态机更新开销。本引擎严格遵循单线程 LMAX Disrupter 架构设计，在真实的生产环境下（开启 CPU 绑定与大页内存）性能表现将更加稳定。

