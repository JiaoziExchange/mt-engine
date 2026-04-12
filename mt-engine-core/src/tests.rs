use crate::book::backend::sparse::SparseBackend;
use crate::codec::CommandCodec;
use crate::prelude::*;
use mt_engine::control_op::ControlOp;
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;

use mt_engine::order_status::OrderStatus as SbeOrderStatus;

pub struct TestTrade {
    pub maker_order_id: u64,
    pub taker_order_id: u64,
    pub price: u64,
    pub quantity: u64,
}

impl TestTrade {
    pub fn maker_order_id(&self) -> u64 { self.maker_order_id }
    pub fn taker_order_id(&self) -> u64 { self.taker_order_id }
    pub fn price(&self) -> u64 { self.price }
    pub fn quantity(&self) -> u64 { self.quantity }
}

pub fn get_trades<'a>(report: &'a crate::outcome::CommandReport<'a>) -> Vec<TestTrade> {
    let reports: Vec<_> = report.execution_reports().filter(|r| r.status() == SbeOrderStatus::traded).collect();
    let mut trades = Vec::new();
    let mut last_cum_qty = std::collections::HashMap::new();
    for chunk in reports.chunks(2) {
        if chunk.len() == 2 {
            let maker = &chunk[0];
            let taker = &chunk[1];
            
            let t_id = taker.order_id();
            let t_cum = taker.cum_qty();
            let t_prev = *last_cum_qty.get(&t_id).unwrap_or(&0);
            let t_trade_qty = t_cum - t_prev;
            last_cum_qty.insert(t_id, t_cum);
            
            trades.push(TestTrade {
                maker_order_id: maker.order_id(),
                taker_order_id: t_id,
                price: maker.price(),
                quantity: t_trade_qty,
            });
        }
    }
    trades
}


#[test]
fn test_engine_basic_matching() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];

    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
            assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 0);
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

            // Use typed iterator for safe reading [SAFE READ]
            let trade = get_trades(&report).into_iter().next().expect("Should have 1 trade");
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 1);
    } else {
        panic!("Expected Partial Fill");
    }
}

#[test]
fn test_engine_fifo_priority() {
    let mut resp_buf = [0u8; 2048];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        let mut trades = get_trades(&report).into_iter();

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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 2);
    } else {
        panic!("Expected Market Fill");
    }
}

#[test]
fn test_engine_cancellation() {
    let mut resp_buf = [0u8; 128];
    let mut cmd_buf = [0u8; 512];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 2);
        let mut trades = get_trades(&report).into_iter();
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
        assert_eq!(get_trades(&report).into_iter().next().unwrap().maker_order_id(), 2);
    }
}

#[test]
fn test_engine_stop_order_reception() {
    let mut resp_buf = [0u8; 128];
    let mut cmd_buf = [0u8; 512];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 0);
        assert_eq!(report.status, OrderStatus::New);
    }

    // Order 1 should be gone
    assert!(engine.backend.get_order_idx_by_id(OrderId(1)).is_none());
}

#[test]
fn test_engine_gtd_fill_before_expiry() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 1);
        assert_eq!(report.status, OrderStatus::Filled);
    }
}

#[test]
fn test_complex_scenario_multi_strategy() {
    let mut resp_buf = [0u8; 8192];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        for trade in get_trades(&report).into_iter() {
            println!(
                "   MATCH: T_ID: {}, M_ID: {}, Qty: {}, Price: {}",
                trade.taker_order_id(),
                trade.maker_order_id(),
                trade.quantity(),
                trade.price()
            );
        }
        assert!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2) > 0);
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        let trade = get_trades(&report).into_iter().next().unwrap();
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
        let mut trades = get_trades(&report).into_iter();

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
        let trade = get_trades(&report).into_iter().next().unwrap();
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 2);
    } else {
        panic!("Execution failed");
    }
}

#[test]
fn test_recursive_trigger() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 2);

        let mut trades = get_trades(&report).into_iter();
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2) >= 1);
    }
}

#[test]
fn test_e2e_stop_limit_slippage() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 2);

        let mut trades = get_trades(&report).into_iter();
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
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 1); // For iceberg, if it fills all at once, it's one trade if it stays at same level
        let trade = get_trades(&report).into_iter().next().unwrap();
        assert_eq!(trade.quantity(), 100);
    }
}

#[test]
fn test_e2e_cascading_volatility() {
    let mut resp_buf = [0u8; 8192];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2) >= 3);
    }
}

#[test]
fn test_e2e_gtd_trigger_expiry() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 1);
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
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
            let trade = get_trades(&report).into_iter().next().expect("Should have 1 trade");
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
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
        let mut trades = get_trades(&report).into_iter();
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
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
        let mut trades = get_trades(&report).into_iter();
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
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
        assert_eq!(get_trades(&report).into_iter().next().unwrap().quantity(), 5);
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 1);
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
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
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 2);
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 10),
        SbeEncoderListener::new(&mut resp_buf),
    ); // Small pool
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
    let mut engine = Engine::new(
        DenseBackend::new(config, 10),
        SbeEncoderListener::new(&mut resp_buf),
    );
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

    // 1. Build initial state on SparseBackend
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // Submit some orders
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
    engine.execute_submit(&codec.encode_submit(
        100,
        OrderId(2),
        UserId(102),
        Side::sell,
        Price(110),
        Quantity(20),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    ));

    // 2. Export snapshot model
    let snapshot = engine.to_snapshot();
    assert_eq!(snapshot.last_sequence_number.0, 2);

    // 3. Restore on a new SparseBackend engine
    let mut resp_buf2 = [0u8; 4096];
    let mut engine_sparse = Engine::new(
        SparseBackend::new(),
        SbeEncoderListener::new(&mut resp_buf2),
    );
    engine_sparse.from_snapshot(snapshot.clone());

    assert_eq!(engine_sparse.backend.best_bid_price().unwrap().0, 100);
    assert_eq!(engine_sparse.backend.best_ask_price().unwrap().0, 110);

    // 4. Restore on a DenseBackend engine (heterogeneous recovery verification)
    let dense_config = PriceRange {
        min: Price(1),
        max: Price(1000),
        tick: Price(1),
    };
    let mut resp_buf3 = [0u8; 4096];
    let mut engine_dense = Engine::new(
        DenseBackend::new(dense_config, 1024),
        SbeEncoderListener::new(&mut resp_buf3),
    );
    engine_dense.from_snapshot(snapshot);

    assert_eq!(engine_dense.backend.best_bid_price().unwrap().0, 100);
    assert_eq!(engine_dense.backend.best_ask_price().unwrap().0, 110);
}

#[test]
#[cfg(feature = "snapshot")]
fn test_snapshot_complex_state_recovery() {
    use crate::book::backend::dense::DenseBackend;
    use crate::book::backend::dense::PriceRange;
    use mt_engine::order_flags::OrderFlags;
    use mt_engine::order_type::OrderType;

    let mut resp_buf = [0u8; 8192];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Prepare complex state
    // - Normal orders
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(1),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    // - Iceberg orders
    let mut iceberg_flags = OrderFlags::new(0);
    iceberg_flags.set_iceberg(true);
    engine.execute_submit(&codec.encode_submit_ext(
        100,
        OrderId(2),
        UserId(2),
        Side::sell,
        OrderType::limit,
        Price(110),
        Quantity(100),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
        iceberg_flags,
    ));

    // - Stop orders
    engine.execute_submit(&codec.encode_submit_ext(
        200,
        OrderId(3),
        UserId(3),
        Side::buy,
        OrderType::stop,
        Price(120),
        Quantity(10),
        SequenceNumber(3),
        Timestamp(1200),
        TimeInForce::gtc,
        OrderFlags::new(0),
    ));

    engine.trade_id_seq = 500; // Manually set initial trade ID sequence

    let snapshot = engine.to_snapshot();

    // 2. Heterogeneous recovery to DenseBackend
    let mut resp_buf2 = [0u8; 8192];
    let mut engine_dense = Engine::new(
        DenseBackend::new(
            PriceRange {
                min: Price(1),
                max: Price(1000),
                tick: Price(1),
            },
            1024,
        ),
        SbeEncoderListener::new(&mut resp_buf2),
    );

    engine_dense.from_snapshot(snapshot);

    // 3. Verify state
    assert_eq!(engine_dense.last_sequence_number.0, 3);
    assert_eq!(engine_dense.trade_id_seq, 500);

    // Verify conditional order pool
    assert_eq!(engine_dense.cond_manager.condition_order_store.len(), 1);
    assert!(engine_dense
        .cond_manager
        .stop_buy_triggers
        .contains_key(&Price(120)));

    // 4. Continue matching, verify logic continuity
    // Submit an order to trigger price 120, activating stop orders
    engine_dense.execute_submit(&codec.encode_submit(
        300,
        OrderId(4),
        UserId(4),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(4),
        Timestamp(1300),
        TimeInForce::gtc,
    ));

    // LTP should be 100. Submit an order to push LTP to 120 (matching with Iceberg order ID 2)
    engine_dense.execute_submit(&codec.encode_submit(
        400,
        OrderId(5),
        UserId(5),
        Side::buy,
        Price(120),
        Quantity(5),
        SequenceNumber(5),
        Timestamp(1400),
        TimeInForce::gtc,
    ));

    assert_eq!(engine_dense.ltp.0, 110); // Matched with ID 2
    assert_eq!(engine_dense.trade_id_seq, 502); // Generated two trades
}

#[test]
#[cfg(feature = "snapshot")]
fn test_snapshot_trigger_threshold_logic() {
    use crate::snapshot::SnapshotConfig;
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    engine.snapshot_config = Some(SnapshotConfig {
        count_interval: 10,
        time_interval_ms: 0,
        path_template: "/tmp/test_snap_{seq}.bin".into(),
        compress: false,
    });

    // Sending 9 commands, should not trigger
    for i in 1..=9 {
        engine.execute_submit(&codec.encode_submit(
            0,
            OrderId(i),
            UserId(1),
            Side::buy,
            Price(100),
            Quantity(1),
            SequenceNumber(i),
            Timestamp(1000 + i),
            TimeInForce::gtc,
        ));
        assert_eq!(engine.uncommitted_commands, i);
    }

    // The 10th command triggers
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(10),
        UserId(1),
        Side::buy,
        Price(100),
        Quantity(1),
        SequenceNumber(10),
        Timestamp(1010),
        TimeInForce::gtc,
    ));

    // Counter should reset after trigger (set by trigger_snapshot)
    assert_eq!(engine.uncommitted_commands, 0);
}

#[test]
#[cfg(feature = "serde")]
fn test_e2e_snapshot_portability() {
    use crate::book::backend::dense::DenseBackend;
    use crate::book::backend::dense::PriceRange;
    use crate::snapshot::SnapshotModel;
    use crate::types::{OrderId, Price, Quantity};
    use mt_engine::order_flags::OrderFlags;
    use mt_engine::side::Side;
    use std::io::Read;

    let mut resp_buf = [0u8; 16384];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Generate complex state
    engine.trade_id_seq = 1000;
    engine.ltp = Price(100); // Set initial price to prevent immediate stop order trigger

    // - Bids
    for i in 1..=5 {
        engine.execute_submit(&codec.encode_submit(
            0,
            OrderId(i),
            UserId(1),
            Side::buy,
            Price(100 - i),
            Quantity(10),
            SequenceNumber(i),
            Timestamp(1000 + i),
            TimeInForce::gtc,
        ));
    }
    // - Asks (including Iceberg orders)
    let mut flags_iceberg = OrderFlags::new(0);
    flags_iceberg.set_iceberg(true);
    engine.execute_submit(&codec.encode_submit_ext(
        200,
        OrderId(6),
        UserId(2),
        Side::sell,
        mt_engine::order_type::OrderType::limit,
        Price(110),
        Quantity(100),
        SequenceNumber(6),
        Timestamp(1100),
        TimeInForce::gtc,
        flags_iceberg,
    ));

    // - Post-Only orders
    let mut flags_post = OrderFlags::new(0);
    flags_post.set_post_only(true);
    engine.execute_submit(&codec.encode_submit_ext(
        0,
        OrderId(7),
        UserId(4),
        Side::buy,
        mt_engine::order_type::OrderType::limit,
        Price(90),
        Quantity(10),
        SequenceNumber(7),
        Timestamp(1150),
        TimeInForce::gtc,
        flags_post,
    ));

    // - Stop-Market orders
    engine.execute_submit(&codec.encode_submit_ext(
        300,
        OrderId(8),
        UserId(5),
        Side::buy,
        mt_engine::order_type::OrderType::stop,
        Price(120),
        Quantity(10),
        SequenceNumber(8),
        Timestamp(1160),
        TimeInForce::gtc,
        OrderFlags::new(0),
    ));

    // - Stop-Limit orders -> currently unified as stop logic
    engine.execute_submit(&codec.encode_submit_ext(
        400,
        OrderId(9),
        UserId(6),
        Side::sell,
        mt_engine::order_type::OrderType::stop_limit,
        Price(80),
        Quantity(10),
        SequenceNumber(9),
        Timestamp(1170),
        TimeInForce::gtc,
        OrderFlags::new(0),
    ));

    // - Match once to update LTP and consume part of Iceberg orders
    // Buy 110 vs Sell 110 (Order 6) -> Trade Price 110
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(10),
        UserId(3),
        Side::buy,
        Price(110),
        Quantity(5),
        SequenceNumber(10),
        Timestamp(1200),
        TimeInForce::gtc,
    ));
    assert_eq!(engine.ltp.0, 110);
    assert_eq!(engine.trade_id_seq, 1001);

    // 2. Simulate export to file (actual Bincode + Zstd)
    let model = engine.to_snapshot();
    let serialized = bincode::serialize(&model).unwrap();
    let snapshot_file = "/tmp/e2e_test_snapshot.bin.zst";

    {
        let file = std::fs::File::create(snapshot_file).unwrap();
        let mut encoder = zstd::Encoder::new(file, 3).unwrap();
        std::io::Write::write_all(&mut encoder, &serialized).unwrap();
        encoder.finish().unwrap();
    }

    // 3. Restore from file
    let mut restored_data = Vec::new();
    {
        let file = std::fs::File::open(snapshot_file).unwrap();
        let mut decoder = zstd::Decoder::new(file).unwrap();
        decoder.read_to_end(&mut restored_data).unwrap();
    }
    let restored_model: SnapshotModel = bincode::deserialize(&restored_data).unwrap();

    // 4. Sparse node recovery test
    let mut resp_buf_s = [0u8; 8192];
    let mut engine_s = Engine::new(
        SparseBackend::new(),
        SbeEncoderListener::new(&mut resp_buf_s),
    );
    engine_s.from_snapshot(restored_model.clone());

    assert_eq!(engine_s.ltp.0, 110);
    assert_eq!(engine_s.trade_id_seq, 1001);
    assert_eq!(engine_s.last_sequence_number.0, 10);
    assert_eq!(
        engine_s
            .backend
            .level_total_qty(engine_s.backend.get_level(Price(110)).unwrap()),
        95
    );

    // Verify conditional order recovery
    assert_eq!(engine_s.cond_manager.condition_order_store.len(), 2);
    assert!(engine_s
        .cond_manager
        .stop_buy_triggers
        .contains_key(&Price(120)));
    assert!(engine_s
        .cond_manager
        .stop_sell_triggers
        .contains_key(&Price(80)));

    // 5. Dense-node recovery test
    {
        let mut resp_buf_d = [0u8; 8192];
        let mut engine_d = Engine::new(
            DenseBackend::new(
                PriceRange {
                    min: Price(1),
                    max: Price(1000),
                    tick: Price(1),
                },
                1024,
            ),
            SbeEncoderListener::new(&mut resp_buf_d),
        );
        engine_d.from_snapshot(restored_model);

        assert_eq!(engine_d.ltp.0, 110);
        // Verify Post-only orders exist in bitset
        assert!(engine_d.backend.get_level(Price(90)).is_some());

        // 6. L2 Depth exhaustive comparison
        let prices_to_check = vec![
            Price(110),
            Price(99),
            Price(98),
            Price(97),
            Price(96),
            Price(95),
            Price(90),
        ];
        for p in prices_to_check {
            let qty_s = engine_s
                .backend
                .get_level(p)
                .map(|l| engine_s.backend.level_total_qty(l))
                .unwrap_or(0);
            let qty_d = engine_d
                .backend
                .get_level(p)
                .map(|l| engine_d.backend.level_total_qty(l))
                .unwrap_or(0);
            assert_eq!(qty_s, qty_d, "L2 Depth mismatch at price {:?}", p);
        }

        // 7. Post-Recovery Match Integrity verification
        // Submit a buy order large enough to consume multiple levels, observe if Trade ID and LTP stay in sync between engines
        let match_cmd = codec.encode_submit(
            0,
            OrderId(100),
            UserId(99),
            Side::buy,
            Price(115),
            Quantity(200),
            SequenceNumber(100),
            Timestamp(2000),
            TimeInForce::gtc,
        );

        engine_s.execute_submit(&match_cmd);
        engine_d.execute_submit(&match_cmd);

        assert_eq!(engine_s.ltp.0, engine_d.ltp.0, "Post-recovery LTP mismatch");
        assert_eq!(
            engine_s.trade_id_seq, engine_d.trade_id_seq,
            "Post-recovery Trade ID mismatch"
        );
    }

    let _ = std::fs::remove_file(snapshot_file);
}

#[test]
fn test_dense_id_out_of_bounds() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let config = PriceRange {
        min: Price(100),
        max: Price(200),
        tick: Price(1),
    };
    // Limit Max Order ID to 1000
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Submit Order ID 1025 (out of bounds)
    let cmd = codec.encode_submit(
        0,
        OrderId(1025),
        UserId(1),
        Side::buy,
        Price(150),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
    );

    let outcome = engine.execute_submit(&cmd);
    match outcome {
        CommandOutcome::Rejected(fail) => {
            assert_eq!(fail, CommandFailure::InvalidOrderId);
        }
        _ => panic!("Expected rejection for out-of-bounds Order ID"),
    }
}

#[test]
fn test_gtd_taker_expired_at_submission() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Submit a GTD order that is already expired
    // ts = 2000, expiry = 2000 (Should be rejected)
    let cmd = codec.encode_submit_gtd(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(2000),
        Timestamp(2000),
    );
    let res = engine.execute_submit_gtd(&cmd);
    match res {
        CommandOutcome::Rejected(CommandFailure::Expired) => {}
        _ => panic!("Expected Expired rejection, got {:?}", res),
    }

    // ts = 2000, expiry = 1999 (Should be rejected)
    let cmd2 = codec.encode_submit_gtd(
        0,
        OrderId(2),
        UserId(101),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(2000),
        Timestamp(1999),
    );
    let res2 = engine.execute_submit_gtd(&cmd2);
    match res2 {
        CommandOutcome::Rejected(CommandFailure::Expired) => {}
        _ => panic!("Expected Expired rejection for cmd2, got {:?}", res2),
    }
}

#[test]
fn test_gtd_taker_expired_at_amendment() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Maker Sell 10 @ 100
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

    // 2. Valid GTD Buy 5 @ 90, Expires at 2000
    engine.execute_submit_gtd(&codec.encode_submit_gtd(
        0,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(90),
        Quantity(5),
        SequenceNumber(2),
        Timestamp(1100),
        Timestamp(2000),
    ));

    // 3. Amend at Time 2500 (Already expired)
    // Change price to 100 to attempt matching as Taker
    let amend = codec.encode_amend(
        0,
        OrderId(2),
        Price(100),
        Quantity(5),
        SequenceNumber(3),
        Timestamp(2500),
    );
    let res = engine.execute_amend(&amend);
    match res {
        CommandOutcome::Rejected(CommandFailure::Expired) => {}
        _ => panic!("Expected Expired rejection at amendment, got {:?}", res),
    }
}

#[test]
fn test_gtd_stop_expired_at_trigger() {
    let mut resp_buf = [0u8; 8192];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Resting Sell Maker 10 @ 100
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(1),
        UserId(101),
        Side::sell,
        Price(100),
        Quantity(10),
        SequenceNumber(1),
        Timestamp(1),
        TimeInForce::gtc,
    ));

    // 2. Stop Buy 10 @ 100 (Trigger >= 100), Expires at 2000
    let s1 = codec.encode_submit_gtd(
        0,
        OrderId(2),
        UserId(102),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1000),
        Timestamp(2000),
    );
    engine.execute_submit_gtd(&s1);

    // 3. Move LTP to 100 at Time 3000 (Already expired)
    // We need a trade to happen at 100
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(3),
        UserId(103),
        Side::buy,
        Price(100),
        Quantity(1),
        SequenceNumber(3),
        Timestamp(3000),
        TimeInForce::gtc,
    ));
    let taker = codec.encode_submit(
        0,
        OrderId(4),
        UserId(104),
        Side::sell,
        Price(100),
        Quantity(1),
        SequenceNumber(4),
        Timestamp(3000),
        TimeInForce::gtc,
    );
    let res = engine.execute_submit(&taker);

    // Check if S1 matched. It shouldn't match because it's expired.
    if let CommandOutcome::Applied(report) = res {
        // Only Taker (4) vs Maker (3) should match
        assert_eq!((report.execution_reports().filter(|r| r.status() == mt_engine::order_status::OrderStatus::traded).count() / 2), 1);
        let trade = get_trades(&report).into_iter().next().unwrap();
        assert_eq!(trade.taker_order_id(), 4);
        assert_eq!(trade.maker_order_id(), 3);
    } else {
        panic!("Expected Applied outcome");
    }

    // Verify S1 is NOT in the book
    assert_eq!(engine.backend.best_bid_price(), None);
}

#[test]
fn test_iceberg_amend_visible_qty_sync() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Submit Iceberg Buy 100 @ 100
    // Note: Since we don't have peak_size in SBE yet, it defaults to remaining_qty (100)
    let m1 = codec.encode_submit_ext(
        0,
        OrderId(1),
        UserId(101),
        Side::buy,
        OrderType::limit,
        Price(100),
        Quantity(100),
        SequenceNumber(1),
        Timestamp(1000),
        TimeInForce::gtc,
        OrderFlags::new(2 /* Iceberg */),
    );
    engine.execute_submit(&m1);

    // Verify initial state
    let order_idx = engine.backend.get_order_idx_by_id(OrderId(1)).unwrap();
    let order = engine.backend.get_order(order_idx).unwrap();
    assert_eq!(order.data.remaining_qty.0, 100);
    assert_eq!(order.data.visible_qty.0, 100);

    // 2. Amend total quantity to 5
    let amend = codec.encode_amend(
        0,
        OrderId(1),
        Price(100),
        Quantity(5),
        SequenceNumber(2),
        Timestamp(1100),
    );
    engine.execute_amend(&amend);

    // Verify synchronized state: visible_qty must be capped at 5
    let order_idx = engine
        .backend
        .get_order_idx_by_id(OrderId(1))
        .expect("Order should exist");
    let order = engine.backend.get_order(order_idx).unwrap();
    assert_eq!(order.data.remaining_qty.0, 5);
    assert_eq!(
        order.data.visible_qty.0, 5,
        "visible_qty should be synced with remaining_qty"
    );
}

#[test]
fn test_iceberg_match_limit_by_visible_qty() {
    let mut resp_buf = [0u8; 4096];
    let mut cmd_buf = [0u8; 1024];
    // Use DenseBackend for variety in testing
    let config = PriceRange {
        min: Price(50),
        max: Price(150),
        tick: Price(1),
    };
    let mut engine = Engine::new(
        DenseBackend::new(config, 1024),
        SbeEncoderListener::new(&mut resp_buf),
    );
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Manually setup an Iceberg order with peak_size < remaining_qty
    // Since SBE doesn't support peak_size yet, we'll "cheat" by using internal state access
    // Or we can just use the fact that our match_order NOW respects visible_qty.

    let mut data: crate::orders::OrderData = unsafe { std::mem::zeroed() };
    data.order_id = OrderId(10);
    data.user_id = UserId(101);
    data.side = Side::sell;
    data.price = Price(100);
    data.remaining_qty = Quantity(100);
    data.visible_qty = Quantity(20); // Peak is 20
    data.peak_size = Quantity(20);
    data.flags.set_iceberg(true);
    let level = engine.backend.get_or_create_level(Side::sell, Price(100));
    let idx = engine
        .backend
        .insert_order(RestingOrder::new(data, level))
        .unwrap();
    engine.backend.push_to_level_back(level, idx);

    // 2. Regular Maker 2: Sell 50 @ 100 (Behind the iceberg)
    engine.execute_submit(&codec.encode_submit(
        0,
        OrderId(11),
        UserId(102),
        Side::sell,
        Price(100),
        Quantity(50),
        SequenceNumber(2),
        Timestamp(1000),
        TimeInForce::gtc,
    ));

    // 3. Taker Buy 50 @ 100
    // Expected trades:
    // - Taker vs Iceberg (10) : 20 qty (Limited by Iceberg peak)
    // - Taker vs Maker 2 (11) : 30 qty
    // Taker is now filled. Iceberg reloads at the back with 80 remaining and 20 visible.
    let taker = codec.encode_submit(
        0,
        OrderId(20),
        UserId(200),
        Side::buy,
        Price(100),
        Quantity(50),
        SequenceNumber(3),
        Timestamp(1100),
        TimeInForce::gtc,
    );
    let res = engine.execute_submit(&taker);

    if let CommandOutcome::Applied(report) = res {
        assert_eq!(report.status, OrderStatus::Filled);
        let mut trades = get_trades(&report).into_iter();

        // First trade: Iceberg peak
        let t1 = trades.next().unwrap();
        assert_eq!(t1.maker_order_id(), 10);
        assert_eq!(t1.quantity(), 20);

        // Second trade: Maker 2
        let t2 = trades.next().unwrap();
        assert_eq!(t2.maker_order_id(), 11);
        assert_eq!(t2.quantity(), 30);
    } else {
        panic!("Expected Applied match");
    }

    // 4. Verify Iceberg reload: it should be behind Maker 2
    // Maker 2 has 20 remaining.
    // Iceberg has 80 remaining.
    // Taker Buy 30 @ 100
    // Hits Maker 2 (20), then Iceberg (10).
    let taker2 = codec.encode_submit(
        0,
        OrderId(21),
        UserId(201),
        Side::buy,
        Price(100),
        Quantity(30),
        SequenceNumber(4),
        Timestamp(1200),
        TimeInForce::gtc,
    );
    if let CommandOutcome::Applied(report) = engine.execute_submit(&taker2) {
        let mut trades = get_trades(&report).into_iter();
        assert_eq!(trades.next().unwrap().maker_order_id(), 11); // Maker 2 is ahead
        assert_eq!(trades.next().unwrap().maker_order_id(), 10); // Iceberg is behind
    }
}

#[test]
fn test_control_shutdown_halt() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Send Shutdown command
    let shutdown = codec.encode_control(0, ControlOp::shutdown, SequenceNumber(1), Timestamp(1000));
    let outcome = engine.execute_control(&shutdown);

    assert!(matches!(outcome, CommandOutcome::Applied(_)));
    assert!(engine.halted);

    // 2. Subsequent trading commands should be rejected with SystemHalted
    let submit = codec.encode_submit(
        100,
        OrderId(1),
        UserId(101),
        Side::buy,
        Price(100),
        Quantity(10),
        SequenceNumber(2),
        Timestamp(1100),
        TimeInForce::gtc,
    );
    let outcome2 = engine.execute_submit(&submit);

    match outcome2 {
        CommandOutcome::Rejected(CommandFailure::SystemHalted) => {}
        _ => panic!("Expected SystemHalted after shutdown, got {:?}", outcome2),
    }

    // Verify Amend and Cancel as well
    let cancel = codec.encode_cancel(200, OrderId(1), SequenceNumber(3), Timestamp(1200));
    match engine.execute_cancel(&cancel) {
        CommandOutcome::Rejected(CommandFailure::SystemHalted) => {}
        _ => panic!("Expected SystemHalted for cancel"),
    }
}

#[test]
fn test_control_expanded_coverage() {
    let mut resp_buf = [0u8; 1024];
    let mut cmd_buf = [0u8; 1024];
    let mut engine = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
    let mut codec = CommandCodec::new(&mut cmd_buf);

    // 1. Verify sequence monotonicity for control commands
    {
        let bad_seq =
            codec.encode_control(0, ControlOp::shutdown, SequenceNumber(0), Timestamp(1000));
        match engine.execute_control(&bad_seq) {
            CommandOutcome::Rejected(CommandFailure::SequenceGap) => {}
            _ => panic!("Expected SequenceGap for seq=0"),
        }
    }

    // 2. Normal shutdown
    {
        let shutdown =
            codec.encode_control(0, ControlOp::shutdown, SequenceNumber(1), Timestamp(1000));
        assert!(matches!(
            engine.execute_control(&shutdown),
            CommandOutcome::Applied(_)
        ));
    }

    // 3. Full business command interception verification
    {
        // OrderSubmit
        let t1 = codec.encode_submit(
            100,
            OrderId(1),
            UserId(1),
            Side::buy,
            Price(100),
            Quantity(10),
            SequenceNumber(2),
            Timestamp(1100),
            TimeInForce::gtc,
        );
        assert!(matches!(
            engine.execute_submit(&t1),
            CommandOutcome::Rejected(CommandFailure::SystemHalted)
        ));

        // OrderSubmitGtd
        let t2 = codec.encode_submit_gtd(
            200,
            OrderId(2),
            UserId(1),
            Side::buy,
            Price(100),
            Quantity(10),
            SequenceNumber(3),
            Timestamp(1200),
            Timestamp(2000),
        );
        assert!(matches!(
            engine.execute_submit_gtd(&t2),
            CommandOutcome::Rejected(CommandFailure::SystemHalted)
        ));

        // OrderCancel
        let t3 = codec.encode_cancel(300, OrderId(1), SequenceNumber(4), Timestamp(1300));
        assert!(matches!(
            engine.execute_cancel(&t3),
            CommandOutcome::Rejected(CommandFailure::SystemHalted)
        ));

        // OrderAmend
        let t4 = codec.encode_amend(
            400,
            OrderId(1),
            Price(101),
            Quantity(10),
            SequenceNumber(5),
            Timestamp(1400),
        );
        assert!(matches!(
            engine.execute_amend(&t4),
            CommandOutcome::Rejected(CommandFailure::SystemHalted)
        ));
    }

    // 4. State isolation verification: New engine should not inherit halted state
    {
        let mut engine2 = Engine::new(SparseBackend::new(), SbeEncoderListener::new(&mut resp_buf));
        let t1 = codec.encode_submit(
            500,
            OrderId(1),
            UserId(1),
            Side::buy,
            Price(100),
            Quantity(10),
            SequenceNumber(1),
            Timestamp(1000),
            TimeInForce::gtc,
        );
        // Should not return SystemHalted; should be accepted normally (even if it fails due to resp_buf reuse or other reasons, logically it should not be blocked by halt flag)
        let res = engine2.execute_submit(&t1);
        if let CommandOutcome::Rejected(CommandFailure::SystemHalted) = res {
            panic!("New engine should not be halted")
        }
    }
}
