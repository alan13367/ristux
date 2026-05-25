pub mod scheduler;

pub fn init() {
    scheduler::init();
    scheduler::run_cooperative_demo();
}

pub fn on_timer_tick(tick: u64) {
    scheduler::on_timer_tick(tick);
}

pub fn yield_current() {
    scheduler::yield_current();
}

pub fn sleep_current(tick: u64, delta: u64) {
    scheduler::sleep_current(tick, delta);
}
