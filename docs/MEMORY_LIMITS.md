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
- `OrderSize`: 56 Bytes
- `LinkSize`: 16 Bytes
- `LevelSize`: 40 Bytes
- `IndexSize (u32)`: 4 Bytes

---

## Example Allocation Case

| Metric | Value | Memory Usage |
| :--- | :--- | :--- |
| **Active Orders (Capacity)** | 1,000,000 | ~72.0 MB |
| **Price Steps (Depth)** | 1,000,000 | ~40.0 MB |
| **Index Mapping (Pre-allocated)** | 1,000,000 | **~4.0 MB** |
| **Total Core Footprint** | - | **~116.0 MB** |

> [!IMPORTANT]
> **Implicit ID Mapping Contract**: By tying the lookup table directly to the order capacity, the Engine assumes that the OMS ensures `OrderId` values sent to the engine do not exceed the `capacity` for that session. If IDs are not recycled and exceed the capacity, the engine will gracefully reject with `InvalidOrderId`.

## Security Guards

1. **Capacity Guard**: When the internal pool is exhausted, new orders are rejected with `CapacityExceeded`.
2. **ID Mapping Guard**: Monotonicity (`ID > last_id`) is enforced at the `Engine` level, while capacity bounds are enforced via the `id_to_index` lookups.
3. **Price Guard**: Any price outside the pre-configured range is rejected with `InvalidPrice` at the entry point.
