use core::arch::asm;

use crate::sync::spinlock::SpinLock;

static ENTROPY: SpinLock<EntropyPool> = SpinLock::new(EntropyPool::new());

struct EntropyPool {
    key: [u32; 8],
    nonce: [u32; 2],
    counter: u64,
    ready: bool,
    rdrand_available: bool,
}

impl EntropyPool {
    const fn new() -> Self {
        Self {
            key: [
                0x243f_6a88,
                0x85a3_08d3,
                0x1319_8a2e,
                0x0370_7344,
                0xa409_3822,
                0x299f_31d0,
                0x082e_fa98,
                0xec4e_6c89,
            ],
            nonce: [0x4528_21e6, 0x38d0_1377],
            counter: 1,
            ready: false,
            rdrand_available: false,
        }
    }

    fn seed(&mut self, rdrand_available: bool) {
        self.rdrand_available = rdrand_available;
        let stack_addr = &self.counter as *const u64 as u64;
        let seed = [
            rdtsc(),
            crate::time::monotonic_ticks(),
            crate::time::uptime_millis(),
            stack_addr,
            0x7269_7374_7578_726e,
            0x6765_6e65_7261_6c21,
        ];
        for value in seed {
            self.mix(value);
        }
        if rdrand_available {
            for _ in 0..8 {
                if let Some(value) = rdrand64() {
                    self.mix(value);
                }
            }
        }
        self.ready = true;
        self.rekey();
    }

    fn mix(&mut self, value: u64) {
        let mut x = splitmix64(value ^ self.counter.rotate_left(17));
        for word in &mut self.key {
            x = splitmix64(x);
            *word ^= x as u32;
        }
        x = splitmix64(x);
        self.nonce[0] ^= x as u32;
        self.nonce[1] ^= (x >> 32) as u32;
        self.counter = self.counter.wrapping_add(x | 1);
    }

    fn fill(&mut self, output: &mut [u8]) {
        if !self.ready {
            self.seed(cpu_has_rdrand());
        }
        self.mix(rdtsc() ^ crate::time::monotonic_ticks().rotate_left(29));
        if self.rdrand_available {
            if let Some(value) = rdrand64() {
                self.mix(value);
            }
        }

        let mut written = 0usize;
        while written < output.len() {
            let block = self.block();
            self.counter = self.counter.wrapping_add(1);
            let count = (output.len() - written).min(block.len());
            output[written..written + count].copy_from_slice(&block[..count]);
            written += count;
        }
        self.rekey();
    }

    fn rekey(&mut self) {
        let block = self.block();
        self.counter = self.counter.wrapping_add(1);
        for i in 0..8 {
            self.key[i] = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        self.nonce[0] ^= u32::from_le_bytes([block[32], block[33], block[34], block[35]]);
        self.nonce[1] ^= u32::from_le_bytes([block[36], block[37], block[38], block[39]]);
    }

    fn block(&self) -> [u8; 64] {
        let constants = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];
        let mut state = [0u32; 16];
        state[0..4].copy_from_slice(&constants);
        state[4..12].copy_from_slice(&self.key);
        state[12] = self.counter as u32;
        state[13] = (self.counter >> 32) as u32;
        state[14] = self.nonce[0];
        state[15] = self.nonce[1];

        let original = state;
        for _ in 0..10 {
            quarter_round(&mut state, 0, 4, 8, 12);
            quarter_round(&mut state, 1, 5, 9, 13);
            quarter_round(&mut state, 2, 6, 10, 14);
            quarter_round(&mut state, 3, 7, 11, 15);
            quarter_round(&mut state, 0, 5, 10, 15);
            quarter_round(&mut state, 1, 6, 11, 12);
            quarter_round(&mut state, 2, 7, 8, 13);
            quarter_round(&mut state, 3, 4, 9, 14);
        }
        for i in 0..16 {
            state[i] = state[i].wrapping_add(original[i]);
        }

        let mut out = [0u8; 64];
        for i in 0..16 {
            out[i * 4..i * 4 + 4].copy_from_slice(&state[i].to_le_bytes());
        }
        out
    }
}

pub fn init() {
    let rdrand = cpu_has_rdrand();
    ENTROPY.lock().seed(rdrand);
    crate::println!(
        "Entropy pool initialized with ChaCha DRBG{}.",
        if rdrand { " and RDRAND" } else { "" }
    );
}

pub fn mix_interrupt_sample(value: u64) {
    ENTROPY
        .lock()
        .mix(value ^ rdtsc() ^ crate::time::monotonic_ticks().rotate_left(11));
}

pub fn fill_random(output: &mut [u8]) {
    ENTROPY.lock().fill(output);
}

fn quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(12);
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(7);
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(target_arch = "x86_64")]
fn cpu_has_rdrand() -> bool {
    let cpuid = core::arch::x86_64::__cpuid(1);
    cpuid.ecx & (1 << 30) != 0
}

#[cfg(not(target_arch = "x86_64"))]
fn cpu_has_rdrand() -> bool {
    false
}

fn rdrand64() -> Option<u64> {
    if !cpu_has_rdrand() {
        return None;
    }
    for _ in 0..8 {
        let value: u64;
        let ok: u8;
        unsafe {
            asm!(
                "rdrand {value}",
                "setc {ok}",
                value = out(reg) value,
                ok = out(reg_byte) ok,
                options(nostack)
            );
        }
        if ok != 0 {
            return Some(value);
        }
    }
    None
}

fn rdtsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}
