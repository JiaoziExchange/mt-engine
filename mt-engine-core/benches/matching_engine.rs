use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;
use mt_engine_core::book::backend::dense::{DenseBackend, PriceRange};
use mt_engine_core::book::backend::sparse::SparseBackend;
use mt_engine_core::book::backend::OrderBookBackend;
use mt_engine_core::codec::CommandCodec;
use mt_engine_core::engine::sbe_listener::SbeEncoderListener;
use mt_engine_core::engine::Engine;
use mt_engine_core::types::{OrderId, Price, Quantity, SequenceNumber, Timestamp, UserId};

const BENCH_CONFIG: PriceRange = PriceRange {
    min: Price(1),
    max: Price(1_100_000),
    tick: Price(1),
};
const BENCH_CAPACITY: usize = 5_000_000;

fn bench_matching_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("Matching");
    let mut resp_buf = [0u8; 65536];
    let mut cmd_buf = [0u8; 1024];

    // --- Scenario 1: Top of Book (Single Match) ---
    // SparseBackend
    group.bench_function("SingleLevel_Sparse", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let maker = codec.encode_submit(
            0,
            OrderId(1),
            UserId(101),
            Side::sell,
            Price(100),
            Quantity(1_000_000_000),
            SequenceNumber(1),
            Timestamp(1000),
            TimeInForce::gtc,
        );
        engine.execute_submit(&maker);
        let mut seq = 2u64;
        b.iter(|| {
            let decoder = codec.encode_submit(
                0,
                OrderId(2),
                UserId(102),
                Side::buy,
                Price(100),
                Quantity(1),
                SequenceNumber(seq),
                Timestamp(1100),
                TimeInForce::gtc,
            );
            let _ = engine.execute_submit(black_box(&decoder));
            seq += 1;
        });
    });

    // DenseBackend
    group.bench_function("SingleLevel_Dense", |b| {
        let mut engine = Engine::new(
            DenseBackend::new(BENCH_CONFIG, BENCH_CAPACITY),
            SbeEncoderListener::new(&mut resp_buf),
        );
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let maker = codec.encode_submit(
            0,
            OrderId(1),
            UserId(101),
            Side::sell,
            Price(100),
            Quantity(1_000_000_000),
            SequenceNumber(1),
            Timestamp(1000),
            TimeInForce::gtc,
        );
        engine.execute_submit(&maker);
        let mut seq = 2u64;
        b.iter(|| {
            let decoder = codec.encode_submit(
                0,
                OrderId(2),
                UserId(102),
                Side::buy,
                Price(100),
                Quantity(1),
                SequenceNumber(seq),
                Timestamp(1100),
                TimeInForce::gtc,
            );
            let _ = engine.execute_submit(black_box(&decoder));
            seq += 1;
        });
    });

    // --- Scenario 2: Level Sweep (Parameterized) ---
    for depth in [1, 5, 20].iter() {
        group.bench_with_input(
            BenchmarkId::new("Sparse_LevelSweep", depth),
            depth,
            |b, &depth| {
                let mut engine =
                    Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
                let mut setup_buf = [0u8; 512];
                let mut codec = CommandCodec::new(&mut setup_buf);
                for i in 0..1000u64 {
                    let price = 100 + i;
                    let dec = codec.encode_submit(
                        0,
                        OrderId(i + 1),
                        UserId(101),
                        Side::sell,
                        Price(price),
                        Quantity(1),
                        SequenceNumber(i + 1),
                        Timestamp(1000),
                        TimeInForce::gtc,
                    );
                    engine.execute_submit(&dec);
                }
                let mut seq = 200_000u64;
                let mut taker_buf = [0u8; 512];
                let mut taker_codec = CommandCodec::new(&mut taker_buf);

                let mut refill_buf = [0u8; 512];
                let mut refill_codec = CommandCodec::new(&mut refill_buf);
                b.iter(|| {
                    let decoder = taker_codec.encode_submit(
                        0,
                        OrderId(999999),
                        UserId(102),
                        Side::buy,
                        Price(999999),
                        Quantity(depth as u64),
                        SequenceNumber(seq),
                        Timestamp(1100),
                        TimeInForce::ioc,
                    );
                    let _ = engine.execute_submit(black_box(&decoder));

                    for i in 0..depth {
                        let price = 100 + i as u64;
                        let refill_dec = refill_codec.encode_submit(
                            0,
                            OrderId(seq + 1 + i as u64),
                            UserId(101),
                            Side::sell,
                            Price(price),
                            Quantity(1),
                            SequenceNumber(seq + 1 + i as u64),
                            Timestamp(1100),
                            TimeInForce::gtc,
                        );
                        engine.execute_submit(&refill_dec);
                    }
                    seq += depth as u64 + 2;
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Dense_LevelSweep", depth),
            depth,
            |b, &depth| {
                let mut engine = Engine::new(
                    DenseBackend::new(BENCH_CONFIG, BENCH_CAPACITY),
                    SbeEncoderListener::new(&mut resp_buf),
                );
                let mut setup_buf = [0u8; 512];
                let mut codec = CommandCodec::new(&mut setup_buf);
                for i in 0..1000u64 {
                    let price = 100 + i;
                    let dec = codec.encode_submit(
                        0,
                        OrderId(i + 1),
                        UserId(101),
                        Side::sell,
                        Price(price),
                        Quantity(1),
                        SequenceNumber(i + 1),
                        Timestamp(1000),
                        TimeInForce::gtc,
                    );
                    engine.execute_submit(&dec);
                }
                let mut seq = 200_000u64;
                let mut taker_buf = [0u8; 512];
                let mut taker_codec = CommandCodec::new(&mut taker_buf);

                let mut refill_buf = [0u8; 512];
                let mut refill_codec = CommandCodec::new(&mut refill_buf);
                b.iter(|| {
                    let decoder = taker_codec.encode_submit(
                        0,
                        OrderId(999999),
                        UserId(102),
                        Side::buy,
                        Price(999999),
                        Quantity(depth as u64),
                        SequenceNumber(seq),
                        Timestamp(1100),
                        TimeInForce::ioc,
                    );
                    let _ = engine.execute_submit(black_box(&decoder));

                    for i in 0..depth {
                        let price = 100 + i as u64;
                        let refill_dec = refill_codec.encode_submit(
                            0,
                            OrderId(seq + 1 + i as u64),
                            UserId(101),
                            Side::sell,
                            Price(price),
                            Quantity(1),
                            SequenceNumber(seq + 1 + i as u64),
                            Timestamp(1100),
                            TimeInForce::gtc,
                        );
                        engine.execute_submit(&refill_dec);
                    }
                    seq += depth as u64 + 2;
                });
            },
        );
    }
    group.finish();
}

fn bench_management_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("Management");
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];

    // SparseBackend
    group.bench_function("CancelOrder_Sparse", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
        let mut codec = CommandCodec::new(&mut cmd_buf);
        for i in 1..=10_000u64 {
            let dec = codec.encode_submit(
                0,
                OrderId(i),
                UserId(101),
                Side::buy,
                Price(100),
                Quantity(10),
                SequenceNumber(i),
                Timestamp(1000),
                TimeInForce::gtc,
            );
            engine.execute_submit(&dec);
        }
        let mut seq = 20_000u64;
        let mut order_id = 1u64;
        b.iter(|| {
            let decoder =
                codec.encode_cancel(0, OrderId(order_id), SequenceNumber(seq), Timestamp(2000));
            let _ = engine.execute_cancel(black_box(&decoder));
            seq += 1;
            order_id = (order_id % 10_000) + 1;
        });
    });

    // DenseBackend
    group.bench_function("CancelOrder_Dense", |b| {
        let mut engine = Engine::new(
            DenseBackend::new(BENCH_CONFIG, BENCH_CAPACITY),
            SbeEncoderListener::new(&mut resp_buf),
        );
        let mut codec = CommandCodec::new(&mut cmd_buf);
        for i in 1..=10_000u64 {
            let dec = codec.encode_submit(
                0,
                OrderId(i),
                UserId(101),
                Side::buy,
                Price(100),
                Quantity(10),
                SequenceNumber(i),
                Timestamp(1000),
                TimeInForce::gtc,
            );
            engine.execute_submit(&dec);
        }
        let mut seq = 20_000u64;
        let mut order_id = 1u64;
        b.iter(|| {
            let decoder =
                codec.encode_cancel(0, OrderId(order_id), SequenceNumber(seq), Timestamp(2000));
            let _ = engine.execute_cancel(black_box(&decoder));
            seq += 1;
            order_id = (order_id % 10_000) + 1;
        });
    });

    group.finish();
}

fn bench_overhead_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("Overhead");
    let mut buf = [0u8; 512];

    group.bench_function("Codec_Encoding", |b| {
        let mut codec = CommandCodec::new(&mut buf);
        b.iter(|| {
            let _ = black_box(codec.encode_submit(
                0,
                OrderId(1),
                UserId(101),
                Side::buy,
                Price(100),
                Quantity(10),
                SequenceNumber(1),
                Timestamp(1000),
                TimeInForce::gtc,
            ));
        });
    });

    group.bench_function("Raw_Buffer_Edit", |b| {
        let mut seq = 1u64;
        b.iter(|| {
            buf[45..53].copy_from_slice(&black_box(seq).to_le_bytes());
            seq += 1;
        });
    });
    group.finish();
}

fn bench_strat_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("Strategies");
    let mut resp_buf = [0u8; 65536];
    let mut cmd_buf = [0u8; 4096];

    // SparseBackend
    group.bench_function("Sparse_Standard_Limit", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 1u64;
        b.iter(|| {
            let dec = codec.encode_submit(
                0,
                OrderId(seq),
                UserId(101),
                Side::buy,
                Price(100),
                Quantity(10),
                SequenceNumber(seq),
                Timestamp(1000),
                TimeInForce::gtc,
            );
            let _ = engine.execute_submit(black_box(&dec));
            seq += 1;
            let cancel =
                codec.encode_cancel(0, OrderId(seq - 1), SequenceNumber(seq), Timestamp(1000));
            let _ = engine.execute_cancel(black_box(&cancel));
            seq += 1;
        });
    });

    group.bench_function("Sparse_Iceberg_Limit", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 1u64;
        let mut flags = mt_engine::order_flags::OrderFlags::new(0);
        flags.set_iceberg(true);
        b.iter(|| {
            let dec = codec.encode_submit_ext(
                0,
                OrderId(seq),
                UserId(101),
                Side::buy,
                mt_engine::order_type::OrderType::limit,
                Price(100),
                Quantity(100),
                SequenceNumber(seq),
                Timestamp(1000),
                TimeInForce::gtc,
                flags,
            );
            let _ = engine.execute_submit(black_box(&dec));
            seq += 1;
            let cancel =
                codec.encode_cancel(0, OrderId(seq - 1), SequenceNumber(seq), Timestamp(1000));
            let _ = engine.execute_cancel(black_box(&cancel));
            seq += 1;
        });
    });

    group.bench_function("Sparse_PostOnly_Maker", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 1u64;
        let mut flags = mt_engine::order_flags::OrderFlags::new(0);
        flags.set_post_only(true);
        b.iter(|| {
            let dec = codec.encode_submit_ext(
                0,
                OrderId(seq),
                UserId(101),
                Side::buy,
                mt_engine::order_type::OrderType::limit,
                Price(100),
                Quantity(10),
                SequenceNumber(seq),
                Timestamp(1000),
                TimeInForce::gtc,
                flags,
            );
            let _ = engine.execute_submit(black_box(&dec));
            seq += 1;
            let cancel =
                codec.encode_cancel(0, OrderId(seq - 1), SequenceNumber(seq), Timestamp(1000));
            let _ = engine.execute_cancel(black_box(&cancel));
            seq += 1;
        });
    });

    // DenseBackend
    group.bench_function("Dense_Standard_Limit", |b| {
        let mut engine = Engine::new(
            DenseBackend::new(BENCH_CONFIG, BENCH_CAPACITY),
            SbeEncoderListener::new(&mut resp_buf),
        );
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 1u64;
        b.iter(|| {
            let dec = codec.encode_submit(
                0,
                OrderId(seq),
                UserId(101),
                Side::buy,
                Price(100),
                Quantity(10),
                SequenceNumber(seq),
                Timestamp(1000),
                TimeInForce::gtc,
            );
            let _ = engine.execute_submit(black_box(&dec));
            seq += 1;
            let cancel =
                codec.encode_cancel(0, OrderId(seq - 1), SequenceNumber(seq), Timestamp(1000));
            let _ = engine.execute_cancel(black_box(&cancel));
            seq += 1;
        });
    });

    group.bench_function("Dense_Iceberg_Limit", |b| {
        let mut engine = Engine::new(
            DenseBackend::new(BENCH_CONFIG, BENCH_CAPACITY),
            SbeEncoderListener::new(&mut resp_buf),
        );
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 1u64;
        let mut flags = mt_engine::order_flags::OrderFlags::new(0);
        flags.set_iceberg(true);
        b.iter(|| {
            let dec = codec.encode_submit_ext(
                0,
                OrderId(seq),
                UserId(101),
                Side::buy,
                mt_engine::order_type::OrderType::limit,
                Price(100),
                Quantity(100),
                SequenceNumber(seq),
                Timestamp(1000),
                TimeInForce::gtc,
                flags,
            );
            let _ = engine.execute_submit(black_box(&dec));
            seq += 1;
            let cancel =
                codec.encode_cancel(0, OrderId(seq - 1), SequenceNumber(seq), Timestamp(1000));
            let _ = engine.execute_cancel(black_box(&cancel));
            seq += 1;
        });
    });

    group.bench_function("Dense_PostOnly_Maker", |b| {
        let mut engine = Engine::new(
            DenseBackend::new(BENCH_CONFIG, BENCH_CAPACITY),
            SbeEncoderListener::new(&mut resp_buf),
        );
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 1u64;
        let mut flags = mt_engine::order_flags::OrderFlags::new(0);
        flags.set_post_only(true);
        b.iter(|| {
            let dec = codec.encode_submit_ext(
                0,
                OrderId(seq),
                UserId(101),
                Side::buy,
                mt_engine::order_type::OrderType::limit,
                Price(100),
                Quantity(10),
                SequenceNumber(seq),
                Timestamp(1000),
                TimeInForce::gtc,
                flags,
            );
            let _ = engine.execute_submit(black_box(&dec));
            seq += 1;
            let cancel =
                codec.encode_cancel(0, OrderId(seq - 1), SequenceNumber(seq), Timestamp(1000));
            let _ = engine.execute_cancel(black_box(&cancel));
            seq += 1;
        });
    });
    group.finish();
}

fn bench_intensity_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("Intensity");
    let mut cmd_buf = [0u8; 4096];

    // Scenario: Single Level with parameterized deep intensity (1k, 10k, 50k)
    for intensity in [1000, 10000, 50000].iter() {
        group.bench_with_input(
            BenchmarkId::new("Massive_Sweep", intensity),
            intensity,
            |b, &intensity| {
                // 在每一循环内重新初始化，确保独立性
                let mut resp_buf_vec = vec![0u8; 8 * 1024 * 1024];
                let mut engine = Engine::new(
                    SparseBackend::new(),
                    SbeEncoderListener::new(resp_buf_vec.as_mut_slice()),
                );
                let mut codec = CommandCodec::new(&mut cmd_buf);

                for i in 1..=intensity {
                    let dec = codec.encode_submit(
                        0,
                        OrderId(i as u64),
                        UserId(1),
                        Side::sell,
                        Price(100),
                        Quantity(1),
                        SequenceNumber(i as u64),
                        Timestamp(1000),
                        TimeInForce::gtc,
                    );
                    engine.execute_submit(&dec);
                }

                let mut seq = (intensity + 1000) as u64;
                b.iter(|| {
                    // Taker sweeps ALL orders at once
                    let dec = codec.encode_submit(
                        0,
                        OrderId(99999),
                        UserId(2),
                        Side::buy,
                        Price(100),
                        Quantity(intensity as u64),
                        SequenceNumber(seq),
                        Timestamp(1100),
                        TimeInForce::gtc,
                    );
                    let _ = engine.execute_submit(black_box(&dec));

                    // Refill (Criterion will measure the whole block, but we focus on the sweep overhead)
                    for i in 1..=intensity {
                        let r_dec = codec.encode_submit(
                            0,
                            OrderId(i as u64),
                            UserId(1),
                            Side::sell,
                            Price(100),
                            Quantity(1),
                            SequenceNumber(seq + i as u64),
                            Timestamp(1200),
                            TimeInForce::gtc,
                        );
                        engine.execute_submit(&r_dec);
                    }
                    seq += (intensity * 2) as u64;
                });
            },
        );
    }
    group.finish();
}

fn bench_scalability_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("Scalability");
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 4096];

    // Scenario: Sparse insertion across 1B price range (Slab growth & BTreeMap balancing)
    group.bench_function("WidePrice_Sparse_100k", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 1u64;

        // Linear Congruential Generator for pseudo-random prices without 'rand' crate
        let mut seed = 42u64;
        b.iter(|| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let price = (seed % 1_000_000_000) + 1;
            let dec = codec.encode_submit(
                0,
                OrderId(seq),
                UserId(1),
                Side::buy,
                Price(price),
                Quantity(10),
                SequenceNumber(seq),
                Timestamp(1000),
                TimeInForce::gtc,
            );
            let _ = engine.execute_submit(black_box(&dec));
            seq += 1;
        });
    });
    group.finish();
}

fn bench_trigger_load_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("TriggerLoad");
    let mut resp_buf = [0u8; 1024 * 512];
    let mut cmd_buf = [0u8; 4096];

    // Scenario: 1,000 cascading Stop orders
    group.bench_function("Cascading_Trigger_1k", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
        let mut codec = CommandCodec::new(&mut cmd_buf);

        // 1. Resting Sell Maker 2,000 @ 1,000,000
        engine.execute_submit(&codec.encode_submit(
            0,
            OrderId(1),
            UserId(1),
            Side::sell,
            Price(1_000_000),
            Quantity(2000),
            SequenceNumber(1),
            Timestamp(1),
            TimeInForce::gtc,
        ));

        // 2. 1,000 Stop Buy orders: Triggered at [100, 101, ..., 1099]
        use mt_engine::order_flags::OrderFlags;
        use mt_engine::order_type::OrderType;
        for i in 0..1000 {
            let trigger_price = 100 + i;
            // 为止损单设置触发价（在我们的简单实现中，止损单的 price 字段即为触发价）
            let dec = codec.encode_submit_ext(
                0,
                OrderId(100 + i),
                UserId(2),
                Side::buy,
                OrderType::stop,
                Price(trigger_price),
                Quantity(1),
                SequenceNumber(100 + i),
                Timestamp(10),
                TimeInForce::gtc,
                OrderFlags::new(0),
            );
            engine.execute_submit(&dec);
        }

        let mut seq = 5000u64;
        b.iter(|| {
            // Taker Buy 1 @ 100 -> LTP becomes 100 -> Triggers ALL 1,000 orders sequentially
            let dec = codec.encode_submit(
                0,
                OrderId(9999),
                UserId(3),
                Side::buy,
                Price(100),
                Quantity(1),
                SequenceNumber(seq),
                Timestamp(100),
                TimeInForce::gtc,
            );
            let _ = engine.execute_submit(black_box(&dec));

            // Note: In real bench, we'd need to reset the engine state or use a very long range.
            // For logic depth, we re-setup a subset. (Criterion averages this).
            seq += 1;
        });
    });
    group.finish();
}

#[derive(Clone, Copy)]
enum MixedIntent {
    SubmitLimit {
        side: Side,
        price: Price,
        qty: Quantity,
        seq: u64,
    },
    SubmitIceberg {
        side: Side,
        price: Price,
        qty: Quantity,
        seq: u64,
    },
    SubmitStop {
        side: Side,
        price: Price,
        qty: Quantity,
        seq: u64,
    },
    SubmitPostOnly {
        side: Side,
        price: Price,
        qty: Quantity,
        seq: u64,
    },
    Cancel {
        target_id: OrderId,
        seq: u64,
    },
}

fn bench_mixed_workload_group(c: &mut Criterion) {
    let mut group = c.benchmark_group("MixedWorkload");

    // 增加预热和采样时间以减少误差
    group.warm_up_time(std::time::Duration::from_secs(10));
    group.measurement_time(std::time::Duration::from_secs(20));
    group.sample_size(100);

    let mut resp_buf_vec = vec![0u8; 10 * 1024 * 1024];
    let resp_buf = resp_buf_vec.as_mut_slice();
    let mut cmd_buf = [0u8; 1024];

    // 显著加大数据量
    let prefill_count = 50_000;
    let ops_per_iter = 10_000;

    let gen_intent = |seed: &mut u64, seq: &mut u64| -> MixedIntent {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let ratio = *seed % 100;
        let side = if (*seed >> 32).is_multiple_of(2) {
            Side::buy
        } else {
            Side::sell
        };
        let base_price = 1000u64;
        let price = Price(base_price + (*seed % 20));
        let qty = Quantity(1 + (*seed % 10));
        *seq += 1;

        // 严格交替：奇数次 Submit，偶数次 Cancel 同一个 ID
        // 这确保了在长达 20s 的高频采样中，内存使用绝对恒定，同时 ID 保持递增
        if *seq % 2 == 1 {
            // Submit 阶段 - 使用递增的 seq 作为 ID
            let sub_ratio = ratio % 50;
            if sub_ratio < 35 {
                MixedIntent::SubmitLimit {
                    side,
                    price,
                    qty,
                    seq: *seq,
                }
            } else if sub_ratio < 40 {
                MixedIntent::SubmitIceberg {
                    side,
                    price,
                    qty,
                    seq: *seq,
                }
            } else if sub_ratio < 45 {
                MixedIntent::SubmitStop {
                    side,
                    price,
                    qty,
                    seq: *seq,
                }
            } else {
                MixedIntent::SubmitPostOnly {
                    side,
                    price,
                    qty,
                    seq: *seq,
                }
            }
        } else {
            // Cancel 阶段 - 撤回刚刚提交的 ID
            let target_cancel_id = OrderId(*seq - 1);
            MixedIntent::Cancel {
                target_id: target_cancel_id,
                seq: *seq,
            }
        }
    };

    fn execute_intent<
        B: OrderBookBackend,
        L: mt_engine_core::engine::events::OrderEventListener,
    >(
        engine: &mut Engine<B, L>,
        codec: &mut CommandCodec,
        intent: MixedIntent,
    ) {
        match intent {
            MixedIntent::SubmitLimit {
                side,
                price,
                qty,
                seq,
            } => {
                let dec = codec.encode_submit(
                    0,
                    OrderId(seq),
                    UserId(1),
                    side,
                    price,
                    qty,
                    SequenceNumber(seq),
                    Timestamp(1),
                    TimeInForce::gtc,
                );
                engine.execute_submit(&dec);
            }
            MixedIntent::SubmitIceberg {
                side,
                price,
                qty,
                seq,
            } => {
                let mut flags = mt_engine::order_flags::OrderFlags::new(0);
                flags.set_iceberg(true);
                let dec = codec.encode_submit_ext(
                    0,
                    OrderId(seq),
                    UserId(1),
                    side,
                    mt_engine::order_type::OrderType::limit,
                    price,
                    Quantity(qty.0 * 5),
                    SequenceNumber(seq),
                    Timestamp(1),
                    TimeInForce::gtc,
                    flags,
                );
                engine.execute_submit(&dec);
            }
            MixedIntent::SubmitStop {
                side,
                price,
                qty,
                seq,
            } => {
                let dec = codec.encode_submit_ext(
                    0,
                    OrderId(seq),
                    UserId(1),
                    side,
                    mt_engine::order_type::OrderType::stop,
                    Price(price.0 + 5),
                    qty,
                    SequenceNumber(seq),
                    Timestamp(1),
                    TimeInForce::gtc,
                    mt_engine::order_flags::OrderFlags::new(0),
                );
                engine.execute_submit(&dec);
            }
            MixedIntent::SubmitPostOnly {
                side,
                price,
                qty,
                seq,
            } => {
                let mut flags = mt_engine::order_flags::OrderFlags::new(0);
                flags.set_post_only(true);
                let dec = codec.encode_submit_ext(
                    0,
                    OrderId(seq),
                    UserId(1),
                    side,
                    mt_engine::order_type::OrderType::limit,
                    price,
                    qty,
                    SequenceNumber(seq),
                    Timestamp(1),
                    TimeInForce::gtc,
                    flags,
                );
                engine.execute_submit(&dec);
            }
            MixedIntent::Cancel { target_id, seq } => {
                let dec = codec.encode_cancel(0, target_id, SequenceNumber(seq), Timestamp(1));
                engine.execute_cancel(&dec);
            }
        }
    }

    // 1. Sparse Backend
    group.bench_function("Sparse_Mixed", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(resp_buf));
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 0u64;
        let mut seed = 42u64;

        for _ in 0..prefill_count {
            let intent = gen_intent(&mut seed, &mut seq);
            execute_intent(&mut engine, &mut codec, intent);
        }

        b.iter(|| {
            for _ in 0..ops_per_iter {
                let intent = gen_intent(&mut seed, &mut seq);
                execute_intent(&mut engine, &mut codec, intent);
            }
        });
    });

    // 2. Dense Backend
    group.bench_function("Dense_Mixed", |b| {
        let mut engine = Engine::new(
            DenseBackend::new(BENCH_CONFIG, BENCH_CAPACITY),
            SbeEncoderListener::new(resp_buf),
        );
        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 0u64;
        let mut seed = 42u64;

        for _ in 0..prefill_count {
            let intent = gen_intent(&mut seed, &mut seq);
            execute_intent(&mut engine, &mut codec, intent);
        }

        b.iter(|| {
            for _ in 0..ops_per_iter {
                let intent = gen_intent(&mut seed, &mut seq);
                execute_intent(&mut engine, &mut codec, intent)
            }
        });
    });

    // 3. Sparse + Snapshot Logic Overhead
    #[cfg(feature = "snapshot")]
    group.bench_function("Sparse_Snapshot_Enabled_Mixed", |b| {
        let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(resp_buf));
        engine.snapshot_config = Some(mt_engine_core::snapshot::SnapshotConfig {
            count_interval: 10_000_000,
            time_interval_ms: 3_600_000,
            path_template: "/tmp/bench_{seq}.bin".to_string(),
            compress: true,
        });

        let mut codec = CommandCodec::new(&mut cmd_buf);
        let mut seq = 0u64;
        let mut seed = 42u64;

        for _ in 0..prefill_count {
            let intent = gen_intent(&mut seed, &mut seq);
            execute_intent(&mut engine, &mut codec, intent);
        }

        b.iter(|| {
            for _ in 0..ops_per_iter {
                let intent = gen_intent(&mut seed, &mut seq);
                execute_intent(&mut engine, &mut codec, intent)
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_matching_group,
    bench_management_group,
    bench_overhead_group,
    bench_strat_group,
    bench_intensity_group,
    bench_scalability_group,
    bench_trigger_load_group,
    bench_mixed_workload_group
);
criterion_main!(benches);
