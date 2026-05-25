use core::ptr;

use crate::{
    memory::{
        frame_allocator::FRAME_SIZE,
        paging::{self, PageFlags, PagingError},
    },
    multiboot::FramebufferInfo,
    sync::spinlock::SpinLock,
};

const BUFFER_WIDTH: usize = 320;
const BUFFER_HEIGHT: usize = 180;
const BUFFER_PIXELS: usize = BUFFER_WIDTH * BUFFER_HEIGHT;
const TERMINAL_X: usize = 34;
const TERMINAL_Y: usize = 102;
const TERMINAL_LINE_HEIGHT: usize = 16;
const TERMINAL_ROWS: usize = 4;
const FRAMEBUFFER_TYPE_RGB: u8 = 1;
const FRAMEBUFFER_TYPE_TEXT: u8 = 2;

static STATE: SpinLock<FramebufferState> = SpinLock::new(FramebufferState::empty());
static BACK_BUFFER: SpinLock<BackBuffer> = SpinLock::new(BackBuffer::new());
static TERMINAL: SpinLock<GraphicsTerminal> = SpinLock::new(GraphicsTerminal::new());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramebufferStats {
    pub initialized: bool,
    pub linear: bool,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub pixels_drawn: usize,
    pub glyphs_drawn: usize,
    pub terminal_lines: usize,
    pub windows_drawn: usize,
    pub fb0_writes: usize,
    pub backbuffer_presented: bool,
}

#[derive(Clone, Copy)]
struct RgbLayout {
    red_shift: u8,
    red_bits: u8,
    green_shift: u8,
    green_bits: u8,
    blue_shift: u8,
    blue_bits: u8,
}

#[derive(Clone, Copy)]
struct LinearFramebuffer {
    addr: usize,
    pitch: usize,
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
    layout: RgbLayout,
}

#[derive(Clone, Copy)]
enum FramebufferMode {
    Uninitialized,
    TextFallback { width: u32, height: u32 },
    Linear(LinearFramebuffer),
}

struct FramebufferState {
    mode: FramebufferMode,
    stats: FramebufferStats,
}

struct BackBuffer {
    pixels: [u32; BUFFER_PIXELS],
}

struct GraphicsTerminal {
    cursor_row: usize,
    lines_written: usize,
}

impl BackBuffer {
    const fn new() -> Self {
        Self {
            pixels: [0; BUFFER_PIXELS],
        }
    }

    fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }

    fn pixel_mut(&mut self, x: usize, y: usize) -> Option<&mut u32> {
        if x >= BUFFER_WIDTH || y >= BUFFER_HEIGHT {
            return None;
        }
        self.pixels.get_mut(y * BUFFER_WIDTH + x)
    }
}

impl FramebufferStats {
    const fn empty() -> Self {
        Self {
            initialized: false,
            linear: false,
            width: 0,
            height: 0,
            bpp: 0,
            pixels_drawn: 0,
            glyphs_drawn: 0,
            terminal_lines: 0,
            windows_drawn: 0,
            fb0_writes: 0,
            backbuffer_presented: false,
        }
    }
}

impl FramebufferState {
    const fn empty() -> Self {
        Self {
            mode: FramebufferMode::Uninitialized,
            stats: FramebufferStats::empty(),
        }
    }
}

pub fn init(info: Option<FramebufferInfo>) {
    let mut state = STATE.lock();
    match info {
        Some(info) if info.buffer_type == FRAMEBUFFER_TYPE_RGB && info.bpp >= 24 => {
            map_framebuffer(&info);
            let layout = RgbLayout {
                red_shift: fallback_shift(info.red_field_position, 16),
                red_bits: fallback_mask(info.red_mask_size),
                green_shift: fallback_shift(info.green_field_position, 8),
                green_bits: fallback_mask(info.green_mask_size),
                blue_shift: fallback_shift(info.blue_field_position, 0),
                blue_bits: fallback_mask(info.blue_mask_size),
            };
            state.mode = FramebufferMode::Linear(LinearFramebuffer {
                addr: info.addr as usize,
                pitch: info.pitch as usize,
                width: info.width as usize,
                height: info.height as usize,
                bytes_per_pixel: (info.bpp as usize).div_ceil(8),
                layout,
            });
            state.stats = FramebufferStats {
                initialized: true,
                linear: true,
                width: info.width,
                height: info.height,
                bpp: info.bpp,
                pixels_drawn: 0,
                glyphs_drawn: 0,
                terminal_lines: 0,
                windows_drawn: 0,
                fb0_writes: 0,
                backbuffer_presented: false,
            };
            crate::println!(
                "Framebuffer graphics initialized: {}x{}x{} at {:#x}.",
                info.width,
                info.height,
                info.bpp,
                info.addr
            );
        }
        Some(info) if info.buffer_type == FRAMEBUFFER_TYPE_TEXT => {
            state.mode = FramebufferMode::TextFallback {
                width: info.width,
                height: info.height,
            };
            state.stats = FramebufferStats {
                initialized: true,
                linear: false,
                width: info.width,
                height: info.height,
                bpp: info.bpp,
                pixels_drawn: 0,
                glyphs_drawn: 0,
                terminal_lines: 0,
                windows_drawn: 0,
                fb0_writes: 0,
                backbuffer_presented: false,
            };
            crate::println!(
                "Framebuffer graphics unavailable; using {}x{} VGA text fallback.",
                info.width,
                info.height
            );
        }
        _ => {
            state.mode = FramebufferMode::TextFallback {
                width: 80,
                height: 25,
            };
            state.stats = FramebufferStats {
                initialized: true,
                linear: false,
                width: 80,
                height: 25,
                bpp: 16,
                pixels_drawn: 0,
                glyphs_drawn: 0,
                terminal_lines: 0,
                windows_drawn: 0,
                fb0_writes: 0,
                backbuffer_presented: false,
            };
            crate::println!("Framebuffer graphics unavailable; using VGA text fallback.");
        }
    }
    drop(state);

    self_test();
}

fn map_framebuffer(info: &FramebufferInfo) {
    let size = info.pitch as usize * info.height as usize;
    let start = info.addr as usize & !(FRAME_SIZE - 1);
    let end = align_up(info.addr as usize + size, FRAME_SIZE);

    let mut addr = start;
    while addr < end {
        let result = unsafe { paging::map_page(addr, addr, PageFlags::WRITABLE) };
        match result {
            Ok(()) | Err(PagingError::AlreadyMapped) => {}
            Err(err) => panic!("framebuffer map failed at {:#x}: {}", addr, err),
        }
        addr += FRAME_SIZE;
    }
}

pub fn stats() -> FramebufferStats {
    STATE.lock().stats
}

pub fn write_bytes(bytes: &[u8]) -> usize {
    let mode = {
        let mut state = STATE.lock();
        state.stats.fb0_writes += 1;
        state.mode
    };

    if let FramebufferMode::Linear(framebuffer) = mode {
        for (index, color) in bytes.chunks(3).take(64).enumerate() {
            let red = color.first().copied().unwrap_or(0);
            let green = color.get(1).copied().unwrap_or(0);
            let blue = color.get(2).copied().unwrap_or(0);
            write_pixel(
                framebuffer,
                index % 16,
                index / 16,
                pack(framebuffer.layout, red, green, blue),
            );
        }
    }

    bytes.len()
}

pub fn terminal_write_line(line: &str) {
    let mut terminal = TERMINAL.lock();
    {
        let mut buffer = BACK_BUFFER.lock();
        terminal.write_line(&mut buffer, line);
    }
    let mut state = STATE.lock();
    state.stats.terminal_lines += 1;
    drop(state);
    present_backbuffer();
}

fn self_test() {
    draw_boot_scene();
    terminal_write_line("sh$ help");
    terminal_write_line("init ok");
    terminal_write_line("fb0 online");
    terminal_write_line("window ready");
    write_bytes(&[0x30, 0x90, 0xff, 0x10, 0xd0, 0xa0, 0xff, 0xff, 0xff]);

    let stats = stats();
    if !stats.initialized
        || stats.glyphs_drawn < 20
        || stats.terminal_lines < TERMINAL_ROWS
        || stats.windows_drawn < 2
        || stats.fb0_writes == 0
    {
        panic!("framebuffer graphics self-test failed");
    }

    crate::println!(
        "Framebuffer graphics self-test passed: {}x{}x{}, linear {}, {} glyph(s), {} terminal line(s), {} window(s).",
        stats.width,
        stats.height,
        stats.bpp,
        stats.linear,
        stats.glyphs_drawn,
        stats.terminal_lines,
        stats.windows_drawn
    );
}

fn draw_boot_scene() {
    {
        let mut buffer = BACK_BUFFER.lock();
        buffer.clear(rgb(9, 15, 24));
        fill_rect(&mut buffer, 0, 0, BUFFER_WIDTH, 24, rgb(17, 47, 74));
        fill_rect(
            &mut buffer,
            0,
            BUFFER_HEIGHT - 24,
            BUFFER_WIDTH,
            24,
            rgb(19, 67, 57),
        );
        draw_window(&mut buffer, 22, 42, 276, 52, "RISTUX", rgb(28, 38, 52));
        draw_window(&mut buffer, 22, 100, 276, 66, "TERM", rgb(18, 25, 36));
        draw_text(&mut buffer, 42, 61, "RISTUX", rgb(236, 242, 248));
        draw_text(&mut buffer, 118, 61, "PHASE 31", rgb(112, 223, 176));
    }

    present_backbuffer();
}

fn present_backbuffer() {
    let mode = STATE.lock().mode;
    let framebuffer = match mode {
        FramebufferMode::Linear(framebuffer) => framebuffer,
        FramebufferMode::TextFallback { width, height } => {
            let mut state = STATE.lock();
            state.stats.width = width;
            state.stats.height = height;
            state.stats.backbuffer_presented = true;
            return;
        }
        FramebufferMode::Uninitialized => return,
    };

    let buffer = BACK_BUFFER.lock();
    let mut drawn = 0usize;
    for y in 0..BUFFER_HEIGHT.min(framebuffer.height) {
        for x in 0..BUFFER_WIDTH.min(framebuffer.width) {
            write_pixel(framebuffer, x, y, buffer.pixels[y * BUFFER_WIDTH + x]);
            drawn += 1;
        }
    }

    let mut state = STATE.lock();
    state.stats.pixels_drawn += drawn;
    state.stats.backbuffer_presented = true;
}

fn fill_rect(buffer: &mut BackBuffer, x: usize, y: usize, width: usize, height: usize, color: u32) {
    for row in y..(y + height).min(BUFFER_HEIGHT) {
        for col in x..(x + width).min(BUFFER_WIDTH) {
            if let Some(pixel) = buffer.pixel_mut(col, row) {
                *pixel = color;
            }
        }
    }
}

fn stroke_rect(
    buffer: &mut BackBuffer,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: u32,
) {
    fill_rect(buffer, x, y, width, 1, color);
    fill_rect(buffer, x, y + height.saturating_sub(1), width, 1, color);
    fill_rect(buffer, x, y, 1, height, color);
    fill_rect(buffer, x + width.saturating_sub(1), y, 1, height, color);
}

fn draw_window(
    buffer: &mut BackBuffer,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    title: &str,
    body: u32,
) {
    fill_rect(buffer, x, y, width, height, body);
    fill_rect(buffer, x, y, width, 14, rgb(31, 66, 92));
    stroke_rect(buffer, x, y, width, height, rgb(90, 180, 210));
    draw_text(buffer, x + 8, y + 3, title, rgb(236, 242, 248));
    let mut state = STATE.lock();
    state.stats.windows_drawn += 1;
}

fn draw_text(buffer: &mut BackBuffer, x: usize, y: usize, text: &str, color: u32) {
    let mut cursor = x;
    for ch in text.bytes() {
        draw_glyph(buffer, cursor, y, ch, color);
        cursor += 7;
    }
}

fn draw_glyph(buffer: &mut BackBuffer, x: usize, y: usize, ch: u8, color: u32) {
    let glyph = glyph(ch);
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                fill_rect(buffer, x + col * 2, y + row * 2, 2, 2, color);
            }
        }
    }

    let mut state = STATE.lock();
    state.stats.glyphs_drawn += 1;
}

fn glyph(ch: u8) -> [u8; 7] {
    match ch {
        b'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        b'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        b'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        b'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        b'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        b'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        b'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        b'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        b'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        b'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        b'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        b'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        b'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        b'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        b'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        b'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        b'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        b'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        b'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        b'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        b'a' => [0, 0b01110, 0b00001, 0b01111, 0b10001, 0b10011, 0b01101],
        b'd' => [
            0b00001, 0b00001, 0b01101, 0b10011, 0b10001, 0b10011, 0b01101,
        ],
        b'e' => [0, 0b01110, 0b10001, 0b11111, 0b10000, 0b10000, 0b01110],
        b'f' => [
            0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000,
        ],
        b'h' => [
            0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001,
        ],
        b'i' => [0b00100, 0, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110],
        b'k' => [
            0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        b'l' => [
            0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        b'n' => [0, 0, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001],
        b'o' => [0, 0, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        b'p' => [0, 0, 0b11110, 0b10001, 0b11110, 0b10000, 0b10000],
        b'r' => [0, 0, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000],
        b's' => [0, 0b01111, 0b10000, 0b01110, 0b00001, 0b11110, 0],
        b't' => [
            0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110,
        ],
        b'w' => [0, 0, 0b10001, 0b10001, 0b10101, 0b10101, 0b01010],
        b'y' => [0, 0, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110],
        b'$' => [
            0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100,
        ],
        b'0' => [
            0b01110, 0b10011, 0b10101, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        b'2' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        b'4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        b'5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        b'6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        b'7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        b'8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        b'9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        b'1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        b'3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        b'>' => [
            0b10000, 0b01000, 0b00100, 0b00010, 0b00100, 0b01000, 0b10000,
        ],
        b' ' => [0, 0, 0, 0, 0, 0, 0],
        _ => [
            0b11111, 0b10001, 0b00110, 0b00100, 0b01100, 0b10001, 0b11111,
        ],
    }
}

impl GraphicsTerminal {
    const fn new() -> Self {
        Self {
            cursor_row: 0,
            lines_written: 0,
        }
    }

    fn write_line(&mut self, buffer: &mut BackBuffer, line: &str) {
        if self.cursor_row >= TERMINAL_ROWS {
            self.cursor_row = TERMINAL_ROWS - 1;
            fill_rect(
                buffer,
                TERMINAL_X,
                TERMINAL_Y,
                246,
                TERMINAL_ROWS * TERMINAL_LINE_HEIGHT,
                rgb(18, 25, 36),
            );
        }

        let y = TERMINAL_Y + self.cursor_row * TERMINAL_LINE_HEIGHT;
        fill_rect(
            buffer,
            TERMINAL_X,
            y,
            246,
            TERMINAL_LINE_HEIGHT,
            rgb(18, 25, 36),
        );
        draw_text(buffer, TERMINAL_X + 4, y + 2, line, rgb(167, 230, 190));
        self.cursor_row += 1;
        self.lines_written += 1;
    }
}

fn write_pixel(framebuffer: LinearFramebuffer, x: usize, y: usize, color: u32) {
    if x >= framebuffer.width || y >= framebuffer.height {
        return;
    }

    let offset = y * framebuffer.pitch + x * framebuffer.bytes_per_pixel;
    unsafe {
        match framebuffer.bytes_per_pixel {
            4 => ptr::write_volatile((framebuffer.addr + offset) as *mut u32, color),
            3 => {
                let ptr = (framebuffer.addr + offset) as *mut u8;
                ptr::write_volatile(ptr, color as u8);
                ptr::write_volatile(ptr.add(1), (color >> 8) as u8);
                ptr::write_volatile(ptr.add(2), (color >> 16) as u8);
            }
            _ => {}
        }
    }
}

fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

fn pack(layout: RgbLayout, red: u8, green: u8, blue: u8) -> u32 {
    (component(red, layout.red_bits) << layout.red_shift)
        | (component(green, layout.green_bits) << layout.green_shift)
        | (component(blue, layout.blue_bits) << layout.blue_shift)
}

fn fallback_shift(value: u8, fallback: u8) -> u8 {
    if value == 0 { fallback } else { value }
}

fn fallback_mask(value: u8) -> u8 {
    if value == 0 { 8 } else { value.min(8) }
}

fn component(value: u8, bits: u8) -> u32 {
    if bits >= 8 {
        value as u32
    } else {
        (value >> (8 - bits)) as u32
    }
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}
