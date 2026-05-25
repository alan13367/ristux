use core::{marker::PhantomData, slice, str};

const TAG_END: u32 = 0;
const TAG_CMDLINE: u32 = 1;
const TAG_BOOTLOADER_NAME: u32 = 2;
const TAG_MODULE: u32 = 3;
const TAG_MMAP: u32 = 6;
const TAG_FRAMEBUFFER: u32 = 8;
const TAG_ACPI_OLD: u32 = 14;
const TAG_ACPI_NEW: u32 = 15;

const MEMORY_AVAILABLE: u32 = 1;

#[derive(Clone, Copy)]
pub struct BootInfo {
    addr: usize,
    total_size: u32,
}

impl BootInfo {
    pub unsafe fn load(addr: usize) -> Result<Self, &'static str> {
        if addr == 0 || addr & 0x7 != 0 {
            return Err("Multiboot2 info address is null or unaligned");
        }

        let total_size = unsafe { *(addr as *const u32) };
        if total_size < 16 {
            return Err("Multiboot2 info block is too small");
        }

        Ok(Self { addr, total_size })
    }

    pub fn total_size(&self) -> u32 {
        self.total_size
    }

    pub fn range(&self) -> (usize, usize) {
        (self.addr, self.addr + self.total_size as usize)
    }

    pub fn bootloader_name(&self) -> Option<&'static str> {
        self.find_string(TAG_BOOTLOADER_NAME)
    }

    pub fn command_line(&self) -> Option<&'static str> {
        self.find_string(TAG_CMDLINE)
    }

    pub fn memory_map(&self) -> Option<MemoryMapIter> {
        let tag = self.find_tag(TAG_MMAP)?;
        if tag.size < 16 {
            return None;
        }

        let payload = tag.payload();
        let entry_size = unsafe { *(payload as *const u32) } as usize;
        let entries_start = payload + 8;
        let entries_end = tag.addr + tag.size as usize;

        Some(MemoryMapIter {
            current: entries_start,
            end: entries_end,
            entry_size,
        })
    }

    pub fn modules(&self) -> ModuleIter {
        ModuleIter { tags: self.tags() }
    }

    pub fn framebuffer(&self) -> Option<FramebufferInfo> {
        let tag = self.find_tag(TAG_FRAMEBUFFER)?;
        if tag.size < 32 {
            return None;
        }

        let payload = tag.payload();
        Some(FramebufferInfo {
            addr: unsafe { *(payload as *const u64) },
            pitch: unsafe { *((payload + 8) as *const u32) },
            width: unsafe { *((payload + 12) as *const u32) },
            height: unsafe { *((payload + 16) as *const u32) },
            bpp: unsafe { *((payload + 20) as *const u8) },
            buffer_type: unsafe { *((payload + 21) as *const u8) },
            red_field_position: read_optional_u8(tag, 24),
            red_mask_size: read_optional_u8(tag, 25),
            green_field_position: read_optional_u8(tag, 26),
            green_mask_size: read_optional_u8(tag, 27),
            blue_field_position: read_optional_u8(tag, 28),
            blue_mask_size: read_optional_u8(tag, 29),
        })
    }

    pub fn acpi_rsdp(&self) -> Option<AcpiRsdp> {
        self.find_tag(TAG_ACPI_NEW)
            .or_else(|| self.find_tag(TAG_ACPI_OLD))
            .and_then(|tag| {
                if tag.size <= 8 {
                    return None;
                }
                Some(AcpiRsdp {
                    addr: tag.payload(),
                    length: tag.size as usize - 8,
                    revision: unsafe { *((tag.payload() + 15) as *const u8) },
                })
            })
    }

    pub fn print_summary(&self) {
        let (start, end) = self.range();
        crate::println!("Multiboot2 info size: {} bytes", self.total_size());
        crate::println!("Multiboot2 info range: {:#x}..{:#x}", start, end);

        match self.bootloader_name() {
            Some(name) => crate::println!("Bootloader: {}", name),
            None => crate::println!("Bootloader: <not provided>"),
        }

        match self.command_line() {
            Some(cmdline) => crate::println!("Kernel command line: {}", cmdline),
            None => crate::println!("Kernel command line: <not provided>"),
        }

        match self.framebuffer() {
            Some(framebuffer) => crate::println!(
                "Framebuffer: {:#x}, {}x{}x{}, pitch {}, type {}",
                framebuffer.addr,
                framebuffer.width,
                framebuffer.height,
                framebuffer.bpp,
                framebuffer.pitch,
                framebuffer.buffer_type
            ),
            None => crate::println!("Framebuffer: <not provided>"),
        }

        match self.acpi_rsdp() {
            Some(rsdp) => crate::println!(
                "ACPI RSDP: {:#x}, {} bytes, revision {}",
                rsdp.addr,
                rsdp.length,
                rsdp.revision
            ),
            None => crate::println!("ACPI RSDP: <not provided>"),
        }

        let mut module_count = 0usize;
        for module in self.modules() {
            module_count += 1;
            crate::println!(
                "Module {}: {:#x}..{:#x} {}",
                module_count,
                module.start,
                module.end,
                module.command_line
            );
            if module.command_line.contains("initrd") {
                crate::println!("Initrd module detected.");
            }
        }

        if module_count == 0 {
            crate::println!("No Multiboot2 modules detected.");
        }

        match self.memory_map() {
            Some(entries) => {
                crate::println!("Memory map:");
                for entry in entries {
                    crate::println!(
                        "  {:#018x}..{:#018x} {}",
                        entry.base_addr,
                        entry.base_addr + entry.length,
                        entry.kind_name()
                    );
                }
            }
            None => crate::println!("Memory map: <not provided>"),
        }
    }

    fn tags(&self) -> TagIter<'static> {
        TagIter {
            current: self.addr + 8,
            end: self.addr + self.total_size as usize,
            done: false,
            _marker: PhantomData,
        }
    }

    fn find_tag(&self, typ: u32) -> Option<Tag<'static>> {
        self.tags().find(|tag| tag.typ == typ)
    }

    fn find_string(&self, typ: u32) -> Option<&'static str> {
        let tag = self.find_tag(typ)?;
        read_c_string(tag.payload(), tag.size as usize - 8)
    }
}

#[derive(Clone, Copy)]
struct Tag<'a> {
    typ: u32,
    size: u32,
    addr: usize,
    _marker: PhantomData<&'a ()>,
}

impl Tag<'_> {
    fn payload(&self) -> usize {
        self.addr + 8
    }
}

struct TagIter<'a> {
    current: usize,
    end: usize,
    done: bool,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Iterator for TagIter<'a> {
    type Item = Tag<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.current + 8 > self.end {
            return None;
        }

        let addr = self.current;
        let typ = unsafe { *(addr as *const u32) };
        let size = unsafe { *((addr + 4) as *const u32) };

        if typ == TAG_END {
            self.done = true;
            return None;
        }

        if size < 8 {
            self.done = true;
            return None;
        }

        self.current = align_up(addr + size as usize, 8);

        Some(Tag {
            typ,
            size,
            addr,
            _marker: PhantomData,
        })
    }
}

pub struct MemoryMapIter {
    current: usize,
    end: usize,
    entry_size: usize,
}

impl Iterator for MemoryMapIter {
    type Item = MemoryMapEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.entry_size < 24 || self.current + self.entry_size > self.end {
            return None;
        }

        let entry = MemoryMapEntry {
            base_addr: unsafe { *(self.current as *const u64) },
            length: unsafe { *((self.current + 8) as *const u64) },
            typ: unsafe { *((self.current + 16) as *const u32) },
        };
        self.current += self.entry_size;
        Some(entry)
    }
}

#[derive(Clone, Copy)]
pub struct MemoryMapEntry {
    pub base_addr: u64,
    pub length: u64,
    pub typ: u32,
}

impl MemoryMapEntry {
    pub fn is_available(&self) -> bool {
        self.typ == MEMORY_AVAILABLE
    }

    pub fn kind_name(&self) -> &'static str {
        if self.is_available() {
            return "available";
        }

        match self.typ {
            2 => "reserved",
            3 => "acpi reclaimable",
            4 => "nvs",
            5 => "badram",
            _ => "unknown",
        }
    }
}

impl Module {
    pub fn size(&self) -> usize {
        self.end.saturating_sub(self.start) as usize
    }

    pub fn bytes(&self) -> &'static [u8] {
        unsafe { slice::from_raw_parts(self.start as *const u8, self.size()) }
    }
}

pub struct ModuleIter {
    tags: TagIter<'static>,
}

impl Iterator for ModuleIter {
    type Item = Module;

    fn next(&mut self) -> Option<Self::Item> {
        for tag in self.tags.by_ref() {
            if tag.typ != TAG_MODULE || tag.size < 16 {
                continue;
            }

            let payload = tag.payload();
            let start = unsafe { *(payload as *const u32) };
            let end = unsafe { *((payload + 4) as *const u32) };
            let command_line = read_c_string(payload + 8, tag.size as usize - 16).unwrap_or("");

            return Some(Module {
                start,
                end,
                command_line,
            });
        }

        None
    }
}

#[derive(Clone, Copy)]
pub struct Module {
    pub start: u32,
    pub end: u32,
    pub command_line: &'static str,
}

#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub addr: u64,
    pub pitch: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub buffer_type: u8,
    pub red_field_position: u8,
    pub red_mask_size: u8,
    pub green_field_position: u8,
    pub green_mask_size: u8,
    pub blue_field_position: u8,
    pub blue_mask_size: u8,
}

#[derive(Clone, Copy)]
pub struct AcpiRsdp {
    pub addr: usize,
    pub length: usize,
    pub revision: u8,
}

fn read_c_string(addr: usize, max_len: usize) -> Option<&'static str> {
    let bytes = unsafe { slice::from_raw_parts(addr as *const u8, max_len) };
    let len = bytes.iter().position(|byte| *byte == 0)?;
    str::from_utf8(&bytes[..len]).ok()
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn read_optional_u8(tag: Tag<'_>, payload_offset: usize) -> u8 {
    let addr = tag.payload() + payload_offset;
    if addr < tag.addr + tag.size as usize {
        unsafe { *(addr as *const u8) }
    } else {
        0
    }
}
