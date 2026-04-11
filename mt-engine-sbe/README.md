# mt_engine (SBE Protocol)

**mt_engine** is the Simple Binary Encoding (SBE) protocol implementation for the [MT-Engine](https://github.com/JiaoziExchange/mt-engine).

It provides high-performance, zero-allocation serialization and deserialization of matching engine commands and market data reports.

## Features
- **Zero-Allocation**: Messages are encoded and decoded directly in-place.
- **Cache-Friendly**: Data structures designed for high CPU cache efficiency.
- **Compact**: Minimal overhead binary format suitable for ultra-low latency trading.

## Usage
Add this to your `Cargo.toml`:
```toml
[dependencies]
mt_engine = "0.1.1"
```

For more information, please visit the [Main Repository](https://github.com/JiaoziExchange/mt-engine).

## License
Licensed under the Apache License, Version 2.0.
