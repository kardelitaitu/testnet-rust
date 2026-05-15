use core_logic::setup_logger;
use tracing::{error, info, warn};

fn main() {
    setup_logger();

    println!("--- Logger Test Start ---");
    info!("Standard INFO (Hidden)");
    warn!("Standard WARN (Hidden)");
    error!("Standard ERROR (Visible)");

    info!(target: "task_result", "TaskResult INFO (Visible)");
    warn!(target: "task_result", "TaskResult WARN (Visible)");
    println!("--- Logger Test End ---");
}
