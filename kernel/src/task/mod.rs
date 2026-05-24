pub mod scheduler;

pub fn init() {
    scheduler::init();
    scheduler::run_cooperative_demo();
}

pub fn on_timer_tick(tick: u64) {
    scheduler::on_timer_tick(tick);
}

