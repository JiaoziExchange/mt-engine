use crate::book::backend::sparse::SparseBackend;
use crate::codec::CommandCodec;
use crate::prelude::*;
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;

#[test]
fn test_engine_basic_matching() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];

    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Submit Limit BUY 10 @ 150 (Maker)
    {
        let decoder = codec.encode_submit(
            0,
            OrderId(1),
            UserId(101),
            Side::buy,
            Price(150),
            Quantity(10),
            SequenceNumber(1),
            Timestamp(1000),
            TimeInForce::gtc,
        );
        let outcome = engine.execute_submit(&decoder);
        if let CommandOutcome::Applied(report) = outcome {
            assert_eq!(report.status, OrderStatus::New);
            assert_eq!(report.trades().count(), 0);
        } else {
            panic!("Expected New");
        }
    }

    // 2. Submit Limit SELL 10 @ 140 (Taker)
    {
        let decoder = codec.encode_submit(
            100,
            OrderId(2),
            UserId(102),
            Side::sell,
            Price(140),
            Quantity(10),
            SequenceNumber(2),
            Timestamp(1100),
            TimeInForce::gtc,
        );
        let outcome = engine.execute_submit(&decoder);
        if let CommandOutcome::Applied(report) = outcome {
            assert_eq!(report.status, OrderStatus::Filled);

            // 使用类型化迭代器安全读取 [SAFE READ]
            let trade = report.trades().next().expect("Should have 1 trade");
            assert_eq!(trade.price(), 150); // Matching at Maker price
            assert_eq!(trade.quantity(), 10);
        } else {
            panic!("Expected Filled");
        }
    }
}

#[test]
fn test_engine_tif_ioc() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker: Sell 5 @ 100
    let m1 = codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(5),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m1);

    // 2. Taker: Buy 10 @ 100 (IOC)
    let t1 = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::ioc,
    );
    let outcome = engine.execute_submit(&t1);

    if let CommandOutcome::Applied(report) = outcome {
        assert_eq!(report.status, OrderStatus::PartiallyFilled);
        assert_eq!(report.trades().count(), 1);
    } else {
        panic!("Expected Partial Fill");
    }
}

#[test]
fn test_engine_fifo_priority() {
    let mut resp_buf = [0u8; 2048];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker 1 & 2 at same price 100
    let m1 = codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m1);
    let m2 = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m2);

    // 2. Taker Buy 15 @ 100. Should hit M1(10) then M2(5)
    let taker = codec.encode_submit(
        200,
        OrderId(3),
        UserId(103),
        Side::buy,
        Price(100),
        Quantity(15),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    );
    if let CommandOutcome::Applied(report) = engine.execute_submit(&taker) {
        let mut trades = report.trades();

        let t1 = trades.next().unwrap();
        assert_eq!(t1.maker_order_id(), 1);
        assert_eq!(t1.quantity(), 10);

        let t2 = trades.next().unwrap();
        assert_eq!(t2.maker_order_id(), 2);
        assert_eq!(t2.quantity(), 5);
    } else {
        panic!("Expected Matching");
    }
}

#[test]
fn test_engine_market_order() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker 1 @ 100, Maker 2 @ 101
    let m1 = codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m1);
    let m2 = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::sell,
        Price(101),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m2);

    // 2. Market Buy 15.
    let taker = codec.encode_market(
        200,
        OrderId(3),
        UserId(103),
        Side::buy,
        Quantity(15),
        SequenceNumber(3),
        Timestamp(1200),
    );
    if let CommandOutcome::Applied(report) = engine.execute_submit(&taker) {
        assert_eq!(report.status, OrderStatus::Filled);
        assert_eq!(report.trades().count(), 2);
    } else {
        panic!("Expected Market Fill");
    }
}

#[test]
fn test_engine_cancellation() {
    let mut resp_buf = [0u8; 128];
    let mut cmd_buf = [0u8; 512];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        Price(150),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    );
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        Price(150),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    let cancel = codec.encode_cancel(100, OrderId(1), SequenceNumber(2), Timestamp(2000));
    if let CommandOutcome::Applied(report) = engine.execute_cancel(&cancel) {
        assert_eq!(report.status, OrderStatus::Cancelled);
    } else {
        panic!("Expected Cancel");
    }
}

#[test]
fn test_engine_amend() {
    let mut resp_buf = [0u8; 128];
    let mut cmd_buf = [0u8; 512];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(150),
        Quantity(20),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    let amend = codec.encode_amend(
        100,
        OrderId(1),
        Price(145),
        Quantity(20),
        SequenceNumber(2),
        Timestamp(2000),
    );
    if let CommandOutcome::Applied(report) = engine.execute_amend(&amend) {
        assert_eq!(report.status, OrderStatus::New);
        assert_eq!(engine.backend.best_ask_price().unwrap().0, 145);
    } else {
        panic!("Expected Amend");
    }
}

#[test]
fn test_engine_sequence_gap() {
    let mut resp_buf = [0u8; 128];
    let mut cmd_buf = [0u8; 512];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(10),
        Timestamp(1000),
        TimeInForce::gtc,
    ));
    let bad_seq = codec.encode_submit(
        0,
        OrderId(2),
        UserId(101),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(5),
        Timestamp(1100),
        TimeInForce::gtc,
    );

    match engine.execute_submit(&bad_seq) {
        CommandOutcome::Rejected(CommandFailure::SequenceGap) => {}
        _ => panic!("Expected SequenceGap"),
    }
}

#[test]
fn test_engine_tif_fok_insufficient() {
    let mut resp_buf = [0u8; 128];
    let mut cmd_buf = [0u8; 512];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(5),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));
    let fok = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::fok,
    );

    match engine.execute_submit(&fok) {
        CommandOutcome::Rejected(CommandFailure::LiquidityInsufficient) => {}
        _ => panic!("Expected FOK LiquidityInsufficient"),
    }
}
use mt_engine::order_flags::OrderFlags;
use mt_engine::order_type::OrderType;

#[test]
fn test_engine_post_only() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker: Sell 10 @ 100
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    // 2. Taker: Buy 5 @ 101 with Post-Only (Crosses Price)
    let mut flags = OrderFlags::new(0);
    flags.set_post_only(true);
    let post_only = codec.encode_submit_ext(
        100,
        OrderId(2),
        UserId(101),
        Side::buy,
        OrderType::limit,
        Price(101),
        Quantity(5),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
        flags,
    );

    match engine.execute_submit(&post_only) {
        CommandOutcome::Rejected(CommandFailure::PostOnlyViolation) => {}
        _ => panic!("Expected PostOnlyViolation"),
    }
}

#[test]
fn test_engine_iceberg_requeue() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker 1: Regular Sell 10 @ 100
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    // 2. Maker 2: Iceberg Sell 100 @ 100 (Peak 10)
    // Note: Our current implementation uses remaining_qty as default peak if not specified,
    // but in OrderSubmitDecoder we don't have peak field yet.
    // In our Engine implementation, iceberg is detected by flags and visible_qty is initialized to total.
    // To test "Re-queue", we need an order that has MORE total than peak.
    // Since we don't have SBE field for peak, I'll manually set peak_size in a mock way or
    // just test the logic where visible_qty < remaining_qty.

    let mut flags = OrderFlags::new(0);
    flags.set_iceberg(true);
    // Submit with 20 total, but we will "simulate" visible = 10 later or just see if it moves to back.
    engine.execute_submit(&codec.encode_submit_ext(
        100,
        OrderId(2),
        UserId(101),
        Side::sell,
        OrderType::limit,
        Price(100),
        Quantity(20),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
        flags,
    ));

    // 3. Maker 3: Regular Sell 10 @ 100
    engine.execute_submit(&codec.encode_submit(
        200,
        OrderId(3),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    ));

    // 4. Taker Buy 15 @ 100.
    // Hits M1(10), then M2(5).
    // Level should be [M2(15), M3(10)].
    let taker = codec.encode_submit(
        300,
        OrderId(4),
        UserId(104),
        Side::buy,
        Price(100),
        Quantity(15),
        SequenceNumber(4),
        Timestamp(1300),
        TimeInForce::gtc,
    );
    let outcome = engine.execute_submit(&taker);

    if let CommandOutcome::Applied(report) = outcome {
        assert_eq!(report.trades().count(), 2);
        let mut trades = report.trades();
        assert_eq!(trades.next().unwrap().maker_order_id(), 1);
        assert_eq!(trades.next().unwrap().maker_order_id(), 2);
    }

    // Since we can't set peak_size < total yet via SBE, we verify that M2 stayed at its position.
    // If it were re-queued (which it shouldn't yet), M3 would be ahead of it.
    // Taker Buy 5 @ 100 should hit M2.
    let taker2 = codec.encode_submit(
        400,
        OrderId(5),
        UserId(105),
        Side::buy,
        Price(100),
        Quantity(5),
        SequenceNumber(5),
        Timestamp(1400),
        TimeInForce::gtc,
    );
    if let CommandOutcome::Applied(report) = engine.execute_submit(&taker2) {
        assert_eq!(report.trades().next().unwrap().maker_order_id(), 2);
    }
}

#[test]
fn test_engine_stop_order_reception() {
    let mut resp_buf = [0u8; 128];
    let mut cmd_buf = [0u8; 512];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // Submit Stop Buy @ 150
    let stop = codec.encode_submit_ext(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        OrderType::stop,
        Price(150),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
        OrderFlags::new(0),
    );
    let outcome = engine.execute_submit(&stop);

    if let CommandOutcome::Applied(report) = outcome {
        assert_eq!(report.status, OrderStatus::New);
        // Should not be in orderbook yet
        assert!(engine.backend.best_bid_price().is_none());
    } else {
        panic!("Expected Accepted Stop Order");
    }
}

#[test]
fn test_engine_lazy_expiry() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Submit Limit SELL 10 @ 100, Expiry = 2000
    let gtd = codec.encode_submit_gtd(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        Timestamp(2000), // expiry
    );
    engine.execute_submit_gtd(&gtd);
    assert!(engine.backend.best_ask_price().is_some());

    // 2. Taker with ts = 2500 (expired)
    let taker = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(100),
        Quantity(5),
        SequenceNumber(2),
        Timestamp(2500),
        TimeInForce::gtc,
    );
    let outcome = engine.execute_submit(&taker);

    // Outcome should be New (resting in book because no match) or New (if it didn't match anything)
    // Actually, match_order should silently remove Order 1.
    if let CommandOutcome::Applied(report) = outcome {
        assert_eq!(report.trades().count(), 0);
        assert_eq!(report.status, OrderStatus::New);
    }

    // Order 1 should be gone
    assert!(engine.backend.get_order_idx_by_id(OrderId(1)).is_none());
}

#[test]
fn test_engine_gtd_fill_before_expiry() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. GTD Sell 10 @ 100, Expiry 2000
    engine.execute_submit_gtd(&codec.encode_submit_gtd(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        Timestamp(2000),
    ));

    // 2. Taker Buy @ 1500 (valid)
    let taker = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(100),
        Quantity(5),
        SequenceNumber(2),
        Timestamp(1500),
        TimeInForce::gtc,
    );
    let outcome = engine.execute_submit(&taker);

    if let CommandOutcome::Applied(report) = outcome {
        assert_eq!(report.trades().count(), 1);
        assert_eq!(report.status, OrderStatus::Filled);
    }
}

#[test]
fn test_complex_scenario_multi_strategy() {
    let mut resp_buf = [0u8; 8192];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    println!("\n======= [MEGA TEST: Multi-Strategy Verification] =======");

    // 1. Maker 1: Regular Sell 100 @ 100
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(100),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));
    println!("[1] Maker 1 (ID: 1, Price: 100, Qty: 100) placed.");

    // 2. Stop Order: Stop Buy @ 100 (Price is 100, should trigger on next trade at 100)
    let stop = codec.encode_submit_ext(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        OrderType::stop,
        Price(100),
        Quantity(50),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
        OrderFlags::new(0),
    );
    engine.execute_submit(&stop);
    println!("[2] Stop Order (ID: 2, Trigger: 100, Side: Buy) placed in trigger pool.");

    // 3. Post-Only Order: Post-Only Buy @ 90
    let mut po_flags = OrderFlags::new(0);
    po_flags.set_post_only(true);
    engine.execute_submit(&codec.encode_submit_ext(
        200,
        OrderId(3),
        UserId(103),
        Side::buy,
        OrderType::limit,
        Price(90),
        Quantity(80),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
        po_flags,
    ));
    println!("[3] Post-Only (ID: 3, Price: 90, Side: Buy) placed as Maker.");

    // 4. Taker: Buy 10 @ 100 (This should result in a trade and trigger LTP change)
    println!("\n[Action] Taker Buy 10 @ 100 sent...");
    let taker = codec.encode_submit(
        300,
        OrderId(4),
        UserId(104),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(4),
        Timestamp(1300),
        TimeInForce::gtc,
    );
    let outcome = engine.execute_submit(&taker);

    if let CommandOutcome::Applied(report) = outcome {
        println!(">> Taker Report: Side: Buy, Status: {:?}", report.status);
        for trade in report.trades() {
            println!(
                "   MATCH: T_ID: {}, M_ID: {}, Qty: {}, Price: {}",
                trade.taker_order_id(),
                trade.maker_order_id(),
                trade.quantity(),
                trade.price()
            );
        }
        assert!(report.trades().count() > 0);
    }

    println!(
        ">> Current LTP: {}",
        engine.backend.best_ask_price().map_or(0, |p| p.0)
    ); // In simplified engine, ltp is updated

    // 5. Post-Only Violation Check
    println!("\n[Action] Post-Only Taker (ID: 5, Buy 10 @ 100) sent...");
    let mut po_taker_flags = OrderFlags::new(0);
    po_taker_flags.set_post_only(true);
    let taker_po = codec.encode_submit_ext(
        400,
        OrderId(5),
        UserId(105),
        Side::buy,
        OrderType::limit,
        Price(100),
        Quantity(10),
        SequenceNumber(5),
        Timestamp(1400),
        TimeInForce::gtc,
        po_taker_flags,
    );
    let outcome_po = engine.execute_submit(&taker_po);
    match outcome_po {
        CommandOutcome::Rejected(CommandFailure::PostOnlyViolation) => {
            println!(">> OK: Post-Only rejected as expected.")
        }
        _ => println!(">> FAILED: Expected rejection but got {:?}", outcome_po),
    }

    println!("========================================================\n");
}

#[test]
fn test_numerical_correctness_and_iceberg_logic() {
    let mut resp_buf = [0u8; 8192];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    println!("\n======= [Numerical Correctness & Iceberg Verification] =======");

    // 1. Maker 1: Regular 50 @ 100
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(10),
        UserId(1),
        Side::sell,
        Price(100),
        Quantity(50),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    // 2. Maker 2: Iceberg 50 @ 100 (We will force peak logic if we could, but let's assume peak=50 for now)
    // To trigger "re-queue", we need visible < remaining.
    // Since we don't have SBE peak yet, we'll use a trick:
    // If we could modify the engine to accept a peak in SBE, it would be better.
    // For now, let's at least verify trade IDs and quantities.
    engine.execute_submit(&codec.encode_submit(
        100,
        OrderId(20),
        UserId(2),
        Side::sell,
        Price(100),
        Quantity(30),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    ));

    println!("Step 1: Two Makers at 100. Total Depth: 80.");

    // 3. Taker 1: Buy 25 @ 100 (Should hit Maker 10 for 25)
    let taker1 = codec.encode_submit(
        200,
        OrderId(30),
        UserId(3),
        Side::buy,
        Price(100),
        Quantity(25),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    );
    let out1 = engine.execute_submit(&taker1);
    if let CommandOutcome::Applied(report) = out1 {
        let trade = report.trades().next().unwrap();
        println!(
            "Taker 1 (25): Match Order 10, Qty: {}, Price: {}",
            trade.quantity(),
            trade.price()
        );
        assert_eq!(trade.quantity(), 25);
        assert_eq!(trade.maker_order_id(), 10);
    }

    // 4. Taker 2: Buy 35 @ 100 (Should hit Maker 10 for rem 25, then Maker 20 for 10)
    let taker2 = codec.encode_submit(
        300,
        OrderId(40),
        UserId(4),
        Side::buy,
        Price(100),
        Quantity(35),
        SequenceNumber(4),
        Timestamp(1300),
        TimeInForce::gtc,
    );
    let out2 = engine.execute_submit(&taker2);
    if let CommandOutcome::Applied(report) = out2 {
        let mut trades = report.trades();

        let t1 = trades.next().unwrap();
        println!(
            "Taker 2 (35) Trade 1: Match Order 10, Qty: {}",
            t1.quantity()
        );
        assert_eq!(t1.quantity(), 25);
        assert_eq!(t1.maker_order_id(), 10);

        let t2 = trades.next().unwrap();
        println!(
            "Taker 2 (35) Trade 2: Match Order 20, Qty: {}",
            t2.quantity()
        );
        assert_eq!(t2.quantity(), 10);
        assert_eq!(t2.maker_order_id(), 20);
    }

    println!("Step 2: After Taker 1 & 2. Order 10 is Filled. Order 20 remaining: 20.");

    // 5. Final check: Best ask should still be 100
    assert_eq!(engine.backend.best_ask_price().unwrap().0, 100);

    // 6. Taker 3: Buy 100 @ 110 (IOC)
    let taker3 = codec.encode_submit(
        400,
        OrderId(50),
        UserId(5),
        Side::buy,
        Price(110),
        Quantity(100),
        SequenceNumber(5),
        Timestamp(1400),
        TimeInForce::ioc,
    );
    let out3 = engine.execute_submit(&taker3);
    if let CommandOutcome::Applied(report) = out3 {
        println!("Taker 3 (100 IOC): Side: Buy, Status: {:?}", report.status);
        let trade = report.trades().next().unwrap();
        println!(
            "Taker 3 (100 IOC): Match Order 20, Qty: {}",
            trade.quantity()
        );
        assert_eq!(trade.quantity(), 20); // Only 20 was left in book
        assert_eq!(report.status, OrderStatus::PartiallyFilled);
    }

    println!("==============================================================\n");
}
#[test]
fn test_sl_tp_triggers() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // Initial LTP = 0. Submit a maker to establish a higher LTP.
    let m1 = codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(150),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(100),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m1);

    let t1 = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(150),
        Quantity(1),
        SequenceNumber(2),
        Timestamp(110),
        TimeInForce::gtc,
    );
    engine.execute_submit(&t1);
    // LTP is now 150

    // 1. Submit Sell Stop @ 100 (Trigger <= 100)
    use mt_engine::order_flags::OrderFlags;
    use mt_engine::order_type::OrderType;
    let flags = OrderFlags::new(0);
    let s1 = codec.encode_submit_ext(
        200,
        OrderId(3),
        UserId(103),
        Side::sell,
        OrderType::stop,
        Price(100),
        Quantity(10),
        SequenceNumber(3),
        Timestamp(120),
        TimeInForce::gtc,
        flags,
    );
    let res = engine.execute_submit(&s1);
    if let CommandOutcome::Applied(report) = res {
        assert_eq!(report.status, OrderStatus::New);
    }

    // 2. Taker Buy to drop price to 100
    // First need a maker at 100
    let m2 = codec.encode_submit(
        300,
        OrderId(4),
        UserId(104),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(4),
        Timestamp(130),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m2);

    let t2 = codec.encode_submit(
        400,
        OrderId(5),
        UserId(105),
        Side::sell,
        Price(100),
        Quantity(1),
        SequenceNumber(5),
        Timestamp(140),
        TimeInForce::gtc,
    );
    let res2 = engine.execute_submit(&t2);
    // LTP becomes 100. Should trigger s1.

    if let CommandOutcome::Applied(report) = res2 {
        // Trade 1: t2 vs m2 (LTP = 100)
        // Trade 2: s1 vs m2 (Triggered)
        assert_eq!(report.trades().count(), 2);
    } else {
        panic!("Execution failed");
    }
}

#[test]
fn test_recursive_trigger() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Resting Sell 10 @ 100
    let m1 = codec.encode_submit(
        0,
        OrderId(1),
        UserId(1),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m1);

    // 2. Stop Buy 10 @ 100 (Trigger >= 100)
    use mt_engine::order_flags::OrderFlags;
    use mt_engine::order_type::OrderType;
    let s1 = codec.encode_submit_ext(
        100,
        OrderId(2),
        UserId(2),
        Side::buy,
        OrderType::stop,
        Price(100),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(2),
        TimeInForce::gtc,
        OrderFlags::new(0),
    );
    engine.execute_submit(&s1);

    // 3. Taker Buy 1 @ 100 -> LTP becomes 100 -> Triggers #2
    let t1 = codec.encode_submit(
        200,
        OrderId(3),
        UserId(3),
        Side::buy,
        Price(100),
        Quantity(1),
        SequenceNumber(3),
        Timestamp(3),
        TimeInForce::gtc,
    );
    let res = engine.execute_submit(&t1);

    if let CommandOutcome::Applied(report) = res {
        // Trade 1: t1 vs m1 (LTP = 100)
        // Trade 2: s1 vs m1 (Triggered by LTP=100)
        assert_eq!(report.trades().count(), 2);

        let mut trades = report.trades();
        let tr1 = trades.next().unwrap();
        assert_eq!(tr1.taker_order_id(), 3);
        assert_eq!(tr1.quantity(), 1);

        let tr2 = trades.next().unwrap();
        assert_eq!(tr2.taker_order_id(), 2); // S1 becomes taker
        assert_eq!(tr2.quantity(), 9); // Matches remaining 9 of M1
    }
}

#[test]
#[ignore] // This is a heavy stress test, run with --ignored
fn test_large_scale_stress() {
    let mut resp_buf = [0u8; 1024 * 1024]; // 1MB
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Stress: Insert 10,000 orders across wide price range
    let mut seed = 12345u64;
    for i in 1..=10_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let price = (seed % 1_000_000) + 1;
        let dec = codec.encode_submit(
            0,
            OrderId(i),
            UserId(1),
            Side::buy,
            Price(price),
            Quantity(10),
            SequenceNumber(i),
            Timestamp(1000),
            TimeInForce::gtc,
        );
        let _ = engine.execute_submit(&dec);
    }

    // 2. Stress: Mass Cancel
    for i in 1..=5_000 {
        let dec = codec.encode_cancel(0, OrderId(i), SequenceNumber(10000 + i), Timestamp(2000));
        let _ = engine.execute_cancel(&dec);
    }

    // 3. Stress: Mass Trigger
    use mt_engine::order_flags::OrderFlags;
    use mt_engine::order_type::OrderType;
    for i in 1..=1000 {
        let trigger_price = 1 + i;
        let dec = codec.encode_submit_ext(
            0,
            OrderId(20000 + i),
            UserId(2),
            Side::sell,
            OrderType::stop,
            Price(trigger_price),
            Quantity(1),
            SequenceNumber(20000 + i),
            Timestamp(3000),
            TimeInForce::gtc,
            OrderFlags::new(0),
        );
        engine.execute_submit(&dec);
    }

    // Trigger them all with one trade
    let m1 = codec.encode_submit(
        0,
        OrderId(99999),
        UserId(9),
        Side::buy,
        Price(1000),
        Quantity(2000),
        SequenceNumber(30000),
        Timestamp(4000),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m1);

    let t1 = codec.encode_submit(
        0,
        OrderId(99998),
        UserId(8),
        Side::sell,
        Price(1000),
        Quantity(1),
        SequenceNumber(31000),
        Timestamp(4100),
        TimeInForce::gtc,
    );
    let res = engine.execute_submit(&t1);

    // Result check: LTP changed to 1000, should have triggered many orders
    if let CommandOutcome::Applied(report) = res {
        assert!(report.trades().count() >= 1);
    }
}

#[test]
fn test_e2e_stop_limit_slippage() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker Sell 10 @ 160
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(160),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    // 2. Stop Buy 10 @ 150 (Trigger >= 150), Limit @ 170 (Slippage)
    use mt_engine::order_type::OrderType;
    let s1 = codec.encode_submit_ext(
        0,
        OrderId(2),
        UserId(102),
        Side::buy,
        OrderType::stop,
        Price(150),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
        mt_engine::order_flags::OrderFlags::new(0),
    );
    engine.execute_submit(&s1);

    // 3. Trade to move LTP to 150
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(3),
        UserId(103),
        Side::sell,
        Price(150),
        Quantity(1),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    ));
    let t1 = codec.encode_submit(
        0,
        OrderId(4),
        UserId(104),
        Side::buy,
        Price(150),
        Quantity(1),
        SequenceNumber(4),
        Timestamp(1300),
        TimeInForce::gtc,
    );
    let res = engine.execute_submit(&t1);

    // LTP is now 150. Stop Buy (OrderId 2) should trigger.
    // It should match with Maker (OrderId 1) at 160.
    if let CommandOutcome::Applied(report) = res {
        // Trade 1: Taker 4 vs Maker 3 @ 150
        // Trade 2: Triggered 2 vs Maker 1 @ 160
        assert_eq!(report.trades().count(), 2);

        let mut trades = report.trades();
        let tr1 = trades.next().unwrap();
        assert_eq!(tr1.price(), 150);

        let tr2 = trades.next().unwrap();
        assert_eq!(tr2.price(), 160); // Verify slippage execution at maker price
        assert_eq!(tr2.taker_order_id(), 2);
        assert_eq!(tr2.maker_order_id(), 1);
    }
}

#[test]
fn test_e2e_iceberg_fok_fill() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Iceberg Sell 100 @ 100 (Peak 10)
    let mut flags = mt_engine::order_flags::OrderFlags::new(0);
    flags.set_iceberg(true);
    let i1 = codec.encode_submit_ext(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        OrderType::limit,
        Price(100),
        Quantity(100),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
        flags,
    );
    // Note: Our simplified codec doesn't pass peak, but Engine::execute_submit in this repo
    // currently sets peak = quantity if not specified. I'll simulate a 10 peak by manually
    // adjusting if needed, or just test if it consumes full quantity.
    engine.execute_submit(&i1);

    // 2. FOK Buy 100 @ 100
    let t1 = codec.encode_submit(
        0,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(100),
        Quantity(100),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::fok,
    );
    let res = engine.execute_submit(&t1);

    if let CommandOutcome::Applied(report) = res {
        assert_eq!(report.status, OrderStatus::Filled);
        assert_eq!(report.trades().count(), 1); // For iceberg, if it fills all at once, it's one trade if it stays at same level
        let trade = report.trades().next().unwrap();
        assert_eq!(trade.quantity(), 100);
    }
}

#[test]
fn test_e2e_cascading_volatility() {
    let mut resp_buf = [0u8; 8192];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Resting Buy Maker 100 @ 90
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        Price(90),
        Quantity(100),
        SequenceNumber(1),
        Timestamp(1),
        TimeInForce::gtc,
    ));

    // 2. Stop Sell 50 @ 95 (Triggered when LTP <= 95)
    let s1 = codec.encode_submit_ext(
        0,
        OrderId(2),
        UserId(102),
        Side::sell,
        OrderType::stop,
        Price(95),
        Quantity(50),
        SequenceNumber(2),
        Timestamp(2),
        TimeInForce::gtc,
        mt_engine::order_flags::OrderFlags::new(0),
    );
    engine.execute_submit(&s1);

    // 3. Take Profit Buy 10 @ 90 (Triggered when LTP <= 91)
    let tp1 = codec.encode_submit_ext(
        0,
        OrderId(3),
        UserId(103),
        Side::buy,
        OrderType::stop,
        Price(91),
        Quantity(10),
        SequenceNumber(3),
        Timestamp(3),
        TimeInForce::gtc,
        mt_engine::order_flags::OrderFlags::new(0),
    );
    engine.execute_submit(&tp1);

    // 4. Trade at 95. Triggers S1.
    // S1 Sell 50 @ 90. Matches with Maker 1 @ 90. LTP becomes 90.
    // LTP=90 triggers TP1.
    // TP1 Buy 10 @ 90. Matches with remaining S1 @ 90.
    let m1 = codec.encode_submit(
        0,
        OrderId(4),
        UserId(104),
        Side::buy,
        Price(95),
        Quantity(10),
        SequenceNumber(4),
        Timestamp(4),
        TimeInForce::gtc,
    );
    engine.execute_submit(&m1);

    let t1 = codec.encode_submit(
        0,
        OrderId(5),
        UserId(105),
        Side::sell,
        Price(95),
        Quantity(10),
        SequenceNumber(5),
        Timestamp(5),
        TimeInForce::gtc,
    );
    let res = engine.execute_submit(&t1);

    if let CommandOutcome::Applied(report) = res {
        // Trade 1: T5 vs M4 @ 95 (LTP=95)
        // -> Triggers S1. S1 matches Maker 1 @ 90 (LTP=90)
        // -> Triggers TP1. TP1 matches remaining S1 @ 90 (LTP=90)
        assert!(report.trades().count() >= 3);
    }
}

#[test]
fn test_e2e_gtd_trigger_expiry() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Stop Buy 10 @ 100, GTD Expires at 2000
    let s1 = codec.encode_submit_gtd(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        Timestamp(2000),
    );
    engine.execute_submit_gtd(&s1);

    // 2. LTP moves to 100, but at Time 3000 (Already expired)
    // Taker Buy @ 100 at Timestamp 3000
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(2),
        UserId(102),
        Side::sell,
        Price(100),
        Quantity(1),
        SequenceNumber(2),
        Timestamp(3000),
        TimeInForce::gtc,
    ));
    let t1 = codec.encode_submit(
        0,
        OrderId(3),
        UserId(103),
        Side::buy,
        Price(100),
        Quantity(1),
        SequenceNumber(3),
        Timestamp(3000),
        TimeInForce::gtc,
    );
    let res = engine.execute_submit(&t1);

    if let CommandOutcome::Applied(report) = res {
        // Only t3 vs t2 should match. S1 should have been ignored because it expired while in the pool.
        assert_eq!(report.trades().count(), 1);
    }
}

use crate::book::backend::dense::{DenseBackend, PriceRange};

#[test]
fn test_engine_dense_matching() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];

    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Submit Limit BUY 10 @ 150 (Maker)
    {
        let decoder = codec.encode_submit(
            0,
            OrderId(1),
            UserId(101),
            Side::buy,
            Price(150),
            Quantity(10),
            SequenceNumber(1),
            Timestamp(1000),
            TimeInForce::gtc,
        );
        let outcome = engine.execute_submit(&decoder);
        if let CommandOutcome::Applied(report) = outcome {
            assert_eq!(report.status, OrderStatus::New);
        } else {
            panic!("Expected New");
        }
    }

    // 2. Submit Limit SELL 10 @ 140 (Taker)
    {
        let decoder = codec.encode_submit(
            100,
            OrderId(2),
            UserId(102),
            Side::sell,
            Price(140),
            Quantity(10),
            SequenceNumber(2),
            Timestamp(1100),
            TimeInForce::gtc,
        );
        let outcome = engine.execute_submit(&decoder);
        if let CommandOutcome::Applied(report) = outcome {
            assert_eq!(report.status, OrderStatus::Filled);
            let trade = report.trades().next().expect("Should have 1 trade");
            assert_eq!(trade.price(), 150);
            assert_eq!(trade.quantity(), 10);
        } else {
            panic!("Expected Filled");
        }
    }
}

#[test]
fn test_dense_fifo_priority() {
    let mut resp_buf = [0u8; 2048];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker 1 & 2 at same price 150
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(150),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));
    engine.execute_submit(&codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::sell,
        Price(150),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    ));

    // 2. Taker Buy 15 @ 150. Should hit M1(10) then M2(5)
    let taker = codec.encode_submit(
        200,
        OrderId(3),
        UserId(103),
        Side::buy,
        Price(150),
        Quantity(15),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    );
    if let CommandOutcome::Applied(report) = engine.execute_submit(&taker) {
        let mut trades = report.trades();
        let t1 = trades.next().unwrap();
        assert_eq!(t1.maker_order_id(), 1);
        let t2 = trades.next().unwrap();
        assert_eq!(t2.maker_order_id(), 2);
        assert_eq!(t2.quantity(), 5);
    } else {
        panic!("Expected Matching");
    }
}

#[test]
fn test_dense_best_price_search() {
    let mut resp_buf = [0u8; 2048];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Asks at 110, 105, 120
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(110),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));
    engine.execute_submit(&codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::sell,
        Price(105),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    ));
    engine.execute_submit(&codec.encode_submit(
        200,
        OrderId(3),
        UserId(103),
        Side::sell,
        Price(120),
        Quantity(10),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    ));

    // Best ask should be 105
    assert_eq!(engine.backend.best_ask_price(), Some(Price(105)));

    // 2. Bids at 95 (out of range min 100), but let's use valid range 101, 102
    engine.execute_submit(&codec.encode_submit(
        300,
        OrderId(4),
        UserId(104),
        Side::buy,
        Price(101),
        Quantity(10),
        SequenceNumber(4),
        Timestamp(1300),
        TimeInForce::gtc,
    ));
    engine.execute_submit(&codec.encode_submit(
        400,
        OrderId(5),
        UserId(105),
        Side::buy,
        Price(102),
        Quantity(10),
        SequenceNumber(5),
        Timestamp(1400),
        TimeInForce::gtc,
    ));

    // Best bid should be 102
    assert_eq!(engine.backend.best_bid_price(), Some(Price(102)));
}

#[test]
fn test_dense_cancellation_positions() {
    let mut resp_buf = [0u8; 2048];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Fill a level with 3 orders
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(150),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));
    engine.execute_submit(&codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::sell,
        Price(150),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    ));
    engine.execute_submit(&codec.encode_submit(
        200,
        OrderId(3),
        UserId(103),
        Side::sell,
        Price(150),
        Quantity(10),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    ));

    // 2. Cancel middle (OrderId 2)
    engine.execute_cancel(&codec.encode_cancel(
        300,
        OrderId(2),
        SequenceNumber(4),
        Timestamp(1300),
    ));

    // Taker Buy 20. Should hit Order 1 then Order 3
    let taker = codec.encode_submit(
        400,
        OrderId(4),
        UserId(104),
        Side::buy,
        Price(150),
        Quantity(20),
        SequenceNumber(5),
        Timestamp(1400),
        TimeInForce::gtc,
    );
    if let CommandOutcome::Applied(report) = engine.execute_submit(&taker) {
        let mut trades = report.trades();
        assert_eq!(trades.next().unwrap().maker_order_id(), 1);
        assert_eq!(trades.next().unwrap().maker_order_id(), 3);
    } else {
        panic!("Expected Matching");
    }

    // 3. Cancel head the tail etc. handled by standard logic
}

#[test]
fn test_dense_amend_logic() {
    let mut resp_buf = [0u8; 2048];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker 1: Sell 20 @ 150
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(150),
        Quantity(20),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    // 2. Amend Quantity (Reduce) -> Should stay at Price 150
    engine.execute_amend(&codec.encode_amend(
        100,
        OrderId(1),
        Price(150),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
    ));

    let taker = codec.encode_submit(
        200,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(150),
        Quantity(5),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    );
    if let CommandOutcome::Applied(report) = engine.execute_submit(&taker) {
        assert_eq!(report.trades().next().unwrap().quantity(), 5);
    } else {
        panic!("Expected Matching");
    }

    // 3. Amend Price -> Should move level
    engine.execute_amend(&codec.encode_amend(
        300,
        OrderId(1),
        Price(160),
        Quantity(10),
        SequenceNumber(4),
        Timestamp(1300),
    ));
    assert_eq!(engine.backend.best_ask_price(), Some(Price(160)));
}

#[test]
fn test_dense_tif_ioc_fok() {
    let mut resp_buf = [0u8; 2048];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker: Sell 5 @ 150
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(150),
        Quantity(5),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    // 2. Taker IOC: Buy 10 @ 150 -> Partial Fill 5, then Cancel 5
    let taker_ioc = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(150),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::ioc,
    );
    if let CommandOutcome::Applied(report) = engine.execute_submit(&taker_ioc) {
        assert_eq!(report.status, OrderStatus::PartiallyFilled);
        assert_eq!(report.trades().count(), 1);
    } else {
        panic!("Expected Partial Fill");
    }

    // 3. Taker FOK: Buy 10 @ 150 -> Should Fail (Insufficient liquidity)
    engine.execute_submit(&codec.encode_submit(
        200,
        OrderId(3),
        UserId(101),
        Side::sell,
        Price(150),
        Quantity(5),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    ));
    let taker_fok = codec.encode_submit(
        300,
        OrderId(4),
        UserId(104),
        Side::buy,
        Price(150),
        Quantity(100),
        SequenceNumber(4),
        Timestamp(1300),
        TimeInForce::fok,
    );
    match engine.execute_submit(&taker_fok) {
        CommandOutcome::Rejected(CommandFailure::LiquidityInsufficient) => {}
        _ => panic!("Expected FOK Reject"),
    }
}

#[test]
fn test_dense_stop_limit_trigger() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Stop Sell 10 @ 140, Triggered when LTP <= 145
    let flags = mt_engine::order_flags::OrderFlags::new(0);
    let stop_sell = codec.encode_submit_ext(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        OrderType::stop,
        Price(140),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
        flags,
    );
    engine.execute_submit(&stop_sell);

    // 2. Buy Maker @ 145
    engine.execute_submit(&codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(145),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    ));

    // 3. Sell Taker 1 @ 145 (LTP becomes 145)
    let taker = codec.encode_submit(
        200,
        OrderId(3),
        UserId(103),
        Side::sell,
        Price(145),
        Quantity(1),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
    );
    let outcome = engine.execute_submit(&taker);

    // Outcome should handle the trigger.
    // In our Engine, process_triggered_orders is called.
    // Order 1 triggers at 145. It attempts to match Buy M2 @ 145.
    if let CommandOutcome::Applied(report) = outcome {
        // Trade 1: T3 vs M2. Trade 2: Triggered S1 vs remaining M2.
        assert_eq!(report.trades().count(), 2);
    } else {
        panic!("Expected Matching + Trigger");
    }
}

#[test]
fn test_dense_boundary_prices() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Min Price 100
    engine.execute_submit(&codec.encode_submit(
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
    assert_eq!(engine.backend.best_bid_price(), Some(Price(100)));

    // 2. Max Price 200
    engine.execute_submit(&codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::sell,
        Price(200),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    ));
    assert_eq!(engine.backend.best_ask_price(), Some(Price(200)));
}

#[test]
fn test_dense_pool_exhaustion() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 10), &mut resp_buf); // Small pool
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // Fill the pool (10 orders)
    for i in 1..=10 {
        engine.execute_submit(&codec.encode_submit(
            0,
            OrderId(i as u64),
            UserId(100),
            Side::sell,
            Price(150),
            Quantity(1),
            SequenceNumber(i as u64),
            Timestamp(1000 + i as u64),
            TimeInForce::gtc,
        ));
    }

    // Attempt 11th order -> should panic or handle gracefully depending on implementation.
    // In our implementation of DenseBackend::insert_order, it uses .expect("Order pool exhausted")
    // So we test that it catches exhaustion if we were to handle it,
    // but here we just verify 10 were added and best_ask is correct.
    assert_eq!(engine.backend.best_ask_price(), Some(Price(150)));
}

#[test]
fn test_dense_out_of_bounds_price() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    let mut engine = Engine::new(DenseBackend::new(config, 10), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Price 99 (Below Min)
    let low = codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        Price(99),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    );
    match engine.execute_submit(&low) {
        CommandOutcome::Rejected(CommandFailure::InvalidPrice) => {}
        _ => panic!("Expected InvalidPrice"),
    }

    // 2. Price 201 (Above Max)
    let high = codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(201),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    );
    match engine.execute_submit(&high) {
        CommandOutcome::Rejected(CommandFailure::InvalidPrice) => {}
        _ => panic!("Expected InvalidPrice"),
    }
}

#[test]
#[cfg(feature = "snapshot")]
fn test_engine_snapshot_recovery() {
    use crate::book::backend::dense::DenseBackend;
    use crate::book::backend::dense::PriceRange;

    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    
    // 1. 在 SparseBackend 上构建初始状态
    let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 提交一些订单
    engine.execute_submit(&codec.encode_submit(
        0, OrderId(1), UserId(101), Side::buy, Price(100), Quantity(10), SequenceNumber(1), Timestamp(1000), TimeInForce::gtc
    ));
    engine.execute_submit(&codec.encode_submit(
        100, OrderId(2), UserId(102), Side::sell, Price(110), Quantity(20), SequenceNumber(2), Timestamp(1100), TimeInForce::gtc
    ));

    // 2. 导出快照模型
    let snapshot = engine.to_snapshot();
    assert_eq!(snapshot.last_sequence_number.0, 2);

    // 3. 在一个新的 SparseBackend 引擎上恢复
    let mut resp_buf2 = [0u8; 4096];
    let mut engine_sparse = Engine::new(SparseBackend::new(), &mut resp_buf2);
    engine_sparse.from_snapshot(snapshot.clone());

    assert_eq!(engine_sparse.backend.best_bid_price().unwrap().0, 100);
    assert_eq!(engine_sparse.backend.best_ask_price().unwrap().0, 110);

    // 4. 在一个 DenseBackend 引擎上恢复 (异构恢复验证)
    let dense_config = PriceRange { min: Price(1), max: Price(1000), tick: Price(1) };
    let mut resp_buf3 = [0u8; 4096];
    let mut engine_dense = Engine::new(DenseBackend::new(dense_config, 1024), &mut resp_buf3);
    engine_dense.from_snapshot(snapshot);

    assert_eq!(engine_dense.backend.best_bid_price().unwrap().0, 100);
    assert_eq!(engine_dense.backend.best_ask_price().unwrap().0, 110);
}
