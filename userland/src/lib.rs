#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

pub mod alloc_impl;
pub mod installer_support;
pub mod io;
pub mod prelude;
pub mod probe_output;
pub mod sys;

/// Slice over a NUL-terminated argv array.
///
/// `argv` is a pointer to an array of `*const u8` C-strings, terminated by a
/// NULL pointer. `argc` is the count (excluding the trailing NULL).
pub fn argv_slice(argc: usize, argv: *const *const u8) -> alloc::vec::Vec<&'static [u8]> {
    use alloc::vec::Vec;
    let mut out = Vec::with_capacity(argc);
    if argv.is_null() {
        return out;
    }
    for i in 0..argc {
        unsafe {
            let p = *argv.add(i);
            if p.is_null() {
                break;
            }
            let mut len = 0usize;
            while *p.add(len) != 0 {
                len += 1;
                if len > 4096 {
                    break;
                }
            }
            out.push(core::slice::from_raw_parts(p, len));
        }
    }
    out
}

/// Defines `_start` for a binary, wiring it to a user-provided `main` returning
/// `i32`. Use as `ristux_userland::program_main!(my_main);` at the bottom of a
/// bin file, where `fn my_main(args: &[&[u8]]) -> i32`.
#[macro_export]
macro_rules! program_main {
    ($main:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn _start(argc: i64, argv: *const *const u8) -> ! {
            let argc = if argc < 0 { 0 } else { argc as usize };
            let args = $crate::argv_slice(argc, argv);
            let arg_refs: alloc::vec::Vec<&[u8]> = args.iter().map(|s| *s).collect();
            let status = $main(&arg_refs);
            $crate::sys::exit(status);
        }
    };
}

use core::panic::PanicInfo;

#[panic_handler]
fn on_panic(info: &PanicInfo) -> ! {
    let _ = sys::write(2, b"userland panic: ");
    if let Some(s) = info.message().as_str() {
        let _ = sys::write(2, s.as_bytes());
    } else {
        let _ = sys::write(2, b"<panic>");
    }
    let _ = sys::write(2, b"\n");
    sys::exit(127);
}

#[alloc_error_handler]
fn on_alloc_error(_layout: core::alloc::Layout) -> ! {
    let _ = sys::write(2, b"userland alloc error\n");
    sys::exit(127);
}
