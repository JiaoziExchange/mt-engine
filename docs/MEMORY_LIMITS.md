# MT-Engine Memory Resource Documentation

This document outlines the deterministic memory footprint of the MT-Engine backends, specifically focusing on the safety-hardened `DenseBackend`.

## Deterministic Resilience

MT-Engine is designed to be OOM-resistant by requiring pre-allocation of all performance-critical structures. In `dense-node` mode, the engine eliminates all non-deterministic hashing.

### Memory Formula (DenseBackend)

The memory usage of `DenseBackend` is fixed at initialization and is now directly tied to the pool capacity, simplifying the resource profile.

$$TotalMemory = (Capacity \times OrderSize) + (Capacity \times LinkSize) + (MaxPriceDepth \times LevelSize) + (Capacity \times IndexSize)$$

#### Key Parameters:
- **Capacity**: Maximum total active orders (Maker).
- **MaxPriceDepth**: Total price steps (ticks) within the configured `PriceRange`.

#### Structural Constants (Internal):
- `OrderSize`: 128 Bytes (Dual cache-line aligned)
- `LinkSize`: 16 Bytes (Dense mode only)
- `LevelSize`: 40 Bytes
- `IndexSize (u32)`: 4 Bytes
- `TriggerNodeSize`: 16 Bytes (Sparse mode condition orders)

---

## Example Allocation Case (Dense Node)

| Metric | Value | Memory Usage |
| :--- | :--- | :--- |
| **Active Orders (Capacity)** | 1,000,000 | **~128.0 MB** |
| **Price Steps (Depth)** | 1,000,000 | ~40.0 MB |
| **Index Mapping (Pre-allocated)** | 1,000,000 | ~4.0 MB |
| **Total Core Footprint** | - | **~172.0 MB** |

## Example Allocation Case (Sparse Node)

The sparse node uses dynamic allocation (Slab) for orders and triggers:
- **Order Store**: ~128MB per 1M active orders.
- **Condition Triggers**: ~16MB per 1M active triggers (Slab pool).

> [!IMPORTANT]
> **Implicit ID Mapping Contract**: By tying the lookup table directly to the order capacity, the Engine assumes that the OMS ensures `OrderId` values sent to the engine do not exceed the `capacity` for that session. If IDs are not recycled and exceed the capacity, the engine will gracefully reject with `InvalidOrderId`.

## Security Guards

1. **Capacity Guard**: When the internal pool is exhausted, new orders are rejected with `CapacityExceeded`.
2. **ID Mapping Guard**: Monotonicity (`ID > last_id`) is enforced at the `Engine` level, while capacity bounds are enforced via the `id_to_index` lookups.
3. **Price Guard**: Any price outside the pre-configured range is rejected with `InvalidPrice` at the entry point.
