# Transaction Types and Order Policies

[English](TRANSACTION_TYPES.md) | [中文](TRANSACTION_TYPES_ZH.md)

This document describes the types of transactions and order execution policies supported by the MT-Engine.

## Command Types (命令类型)

These are the top-level operations that can be sent to the matching engine.

| Command | Description | Chinese Description |
| :--- | :--- | :--- |
| **Submit** | Create and submit a new order to the book. | 下单：创建并提交新订单。 |
| **Cancel** | Cancel an existing resting order by its `order_id`. | 撤单：根据订单 ID 取消挂单。 |
| **Amend** | Modify the price or quantity of an existing resting order. | 改单：修改现有挂单的价格或数量。 |

---

## Order Types (订单类型)

| Type | Description | Status |
| :--- | :--- | :--- |
| **Limit** | An order to buy or sell at a specified price or better. | **Implemented** |
| **Market** | An order to buy or sell at the best available current price. | **Implemented** |
| **Stop** | An order that becomes a market/limit order when a price threshold is reached. | **Implemented** |
| **Stop Limit** | An order that becomes a limit order when a price threshold is reached. | **Implemented** |

---

## Side (买卖方向)

| Side | Description |
| :--- | :--- |
| **Buy** | A bid to purchase an asset. |
| **Sell** | An ask to sell an asset. |

---

## Time In Force (有效期策略/TIF)

TIF policies define how long an order remains active and how it interacts with existing liquidity.

| TIF | Name | Description | Status |
| :--- | :--- | :--- | :--- |
| **GTC** | Good Till Cancel | Remains active until fully filled or manually canceled. | **Implemented** |
| **IOC** | Immediate Or Cancel | Unfilled portion is canceled immediately. | **Implemented** |
| **FOK** | Fill Or Kill | Must be filled entirely and immediately, or canceled. | **Implemented** |
| **GTD** | Good Till Date | Remains active until a specified `expiry_time`. | **Implemented** |
| **GTH** | Good Till Hour | Remains active until a specified hour. | **Implemented** |

---

## Order Flags (订单标志)

| Flag | Description | Engine Logic | Status |
| :--- | :--- | :--- | :--- |
| **Post-Only** | Only accepted if it doesn't match immediately. | **Lazy Validation**: Intercepted at price cross. | **Implemented** |
| **Iceberg** | Hide total quantity, only show peak size. | **FIFO Re-queue**: Re-queued at level back on refresh. | **Implemented** |
| **Reduce-Only**| Only reduce existing position. | **OMS Layer**: Handled by upper layer (OMS). | Out-of-Scope |
| **STP** | Self-Trade Prevention. | **OMS Layer**: Handled by upper layer (OMS). | Out-of-Scope |

---

## Trigger Logic (触发逻辑)

Stop and Stop-Limit orders are managed via an internal **Trigger Pool** (BTreeMap):
- **Trigger Type**: LTP (Last Traded Price).
- **Execution**: Recursive activation loop (Max Depth 10) to handle cascading triggers without stack overflow.
- **Nanosecond Efficiency**: Trigger checks are performed in the post-match phase, outside the critical hot path of the price-time matching loop.
- **Safety**: Iteration limit (Max 10) prevents infinite cascade triggers.

---

## Safety & Performance Design

The MT-Engine is designed for deterministic, high-frequency trading:

### Zero-Allocation (零分配)
- All commands are processed using steady-state buffers.
- Responses (Trade reports) are encoded directly via SBE into a user-provided response buffer.

### Cache Optimization (缓存优化)
- **Hardware Prefetch**: Uses x86_64 `_mm_prefetch` to load order data and condition orders into L1 Cache before they are accessed in the hot path.
- **Slab Allocation**: Orders and levels are stored in dense `Slab` structures to minimize pointer chasing and improve cache locality.

### Defined Safety (确定性安全)
- **Iteration Limits**: Recursive triggers are bounded to prevent stack overflow and CPU starvation.
- **SBE Encoding**: Binary encoding ensures no string parsing overhead and fixed-width fields for predictable performance.

---

## Post-Trade Information

Each successful transaction produces a `Trade` report via SBE:
- `trade_id`: Unique trade identifier.
- `maker_order_id`: ID of the passive order.
- `taker_order_id`: ID of the aggressive order.
- `price`: Execution price.
- `quantity`: Execution quantity.
- `timestamp`: Nanosecond timestamp.
