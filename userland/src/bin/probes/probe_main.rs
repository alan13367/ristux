extern crate alloc;
extern crate ristux_userland;

fn main(args: &[&[u8]]) -> i32 {
    ristux_userland::probe_output::run(args)
}

ristux_userland::program_main!(main);
