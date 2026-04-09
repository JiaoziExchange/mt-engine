use mt_engine_core::orders::OrderData;
use mt_engine_core::engine::TriggerNode;

fn main() {
    println!("OrderData: {} bytes", std::mem::size_of::<OrderData>());
    println!("TriggerNode: {} bytes", std::mem::size_of::<TriggerNode>());
}
