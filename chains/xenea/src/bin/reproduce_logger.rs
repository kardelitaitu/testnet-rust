use core_logic::setup_logger;
use tracing::{error, info};

fn main() {
    setup_logger();
    println!("--- Testing Existing SetupLogger ---");
    info!("Generic INFO (Should be HIDDEN)");
    error!("Generic ERROR (Should be SHOWN)");
    info!(target: "task_result", "Task Result (Should be SHOWN)");
    println!("--- End Test ---");
}
