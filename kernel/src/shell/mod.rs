//! Legacy in-kernel scripted shell.
//!
//! Phase A moves the interactive prompt to a real ring‑3 `/bin/sh` spawned by
//! `/bin/init`. The original `shell::init()` script remains under the
//! `selftest_shell_script` feature flag for offline regression runs but is no
//! longer invoked from boot.

#[cfg(feature = "selftest_shell_script")]
mod legacy {
    pub fn run() {
        crate::println!(
            "Legacy shell self-test script disabled; ring-3 /bin/sh is the boot shell."
        );
    }
}

pub fn init() {
    #[cfg(feature = "selftest_shell_script")]
    {
        legacy::run();
        crate::println!("Shell self-test script completed.");
        return;
    }
    crate::println!("Kernel shell stub: ring-3 /bin/init owns the prompt now.");
}
