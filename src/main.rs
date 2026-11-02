#![no_std]
#![no_main]

use core::fmt::{self, Write};
use core::panic::PanicInfo;

mod arch;
mod boot;
mod display;
mod memory;

#[used]
#[link_section = ".limine_requests_start"]
static LIMINE_REQUESTS_START_MARKER: [u64; 2] = [0xf6b8_f4b3_9de7_d1ae, 0xfab9_1a69_40fc_b9cf];

#[used]
#[link_section = ".limine_requests"]
static LIMINE_BASE_REVISION: [u64; 3] = [0xf956_2b2d_5c95_a6c8, 0x6a7b_3849_4453_6bdc, 3];

#[used]
#[link_section = ".limine_requests_end"]
static LIMINE_REQUESTS_END_MARKER: [u64; 2] = [0xadc0_e053_1bb1_0d03, 0x9572_709f_3176_4c62];

struct SerialPort;

impl SerialPort {
    const COM1: u16 = 0x3f8;

    unsafe fn out(port: u16, value: u8) {
        // SAFETY: COM1 I/O port access is intentional during early boot.
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") port,
                in("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }

    unsafe fn input(port: u16) -> u8 {
        let value: u8;
        // SAFETY: Reading the selected UART register is intentional during early boot.
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") port,
                out("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }

    unsafe fn initialise() {
        // SAFETY: Configure the legacy COM1 UART once, before normal kernel execution.
        unsafe {
            Self::out(Self::COM1 + 1, 0x00);
            Self::out(Self::COM1 + 3, 0x80);
            Self::out(Self::COM1, 0x03);
            Self::out(Self::COM1 + 1, 0x00);
            Self::out(Self::COM1 + 3, 0x03);
            Self::out(Self::COM1 + 2, 0xc7);
            Self::out(Self::COM1 + 4, 0x0b);
        }
    }

    fn write_byte(&mut self, byte: u8) {
        for _ in 0..1_000_000 {
            // SAFETY: Read the COM1 line-status register while polling transmitter readiness.
            if unsafe { Self::input(Self::COM1 + 5) } & 0x20 != 0 {
                // SAFETY: Write one byte to the COM1 transmit register.
                unsafe { Self::out(Self::COM1, byte) };
                return;
            }
            core::hint::spin_loop();
        }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // SAFETY: This is the single early-boot initialisation of COM1.
    unsafe { SerialPort::initialise() };
    let mut serial = SerialPort;
    let _ = writeln!(serial, "");
    let _ = writeln!(serial, "========================================");
    let _ = writeln!(serial, "  SESTINA :: x86_64 kernel");
    let _ = writeln!(serial, "========================================");
    let _ = writeln!(serial, "[boot] Entered kernel entry point");
    let _ = writeln!(serial, "[boot] COM1 serial console initialised");
    let _ = writeln!(serial, "[boot] Limine base revision request present");

    // SAFETY: Early startup is single-core and maskable interrupts remain disabled.
    unsafe { arch::x86_64::idt::initialize() };
    let _ = writeln!(serial, "[cpu] IDT installed (256 gates)");
    let _ = writeln!(serial, "[cpu] Exception diagnostics enabled (vectors 0-31)");

    let cpu = arch::x86_64::cpu::detect();
    let vendor = core::str::from_utf8(&cpu.vendor).unwrap_or("unknown");
    let _ = writeln!(serial, "[cpu] Vendor: {}", vendor);
    let _ = writeln!(serial, "[cpu] CPUID basic leaf: {:#x}", cpu.max_basic_leaf);
    let _ = writeln!(
        serial,
        "[cpu] CPUID extended leaf: {:#x}",
        cpu.max_extended_leaf
    );
    let _ = writeln!(
        serial,
        "[cpu] SSE2: {}, APIC: {}, x2APIC: {}",
        cpu.has_sse2, cpu.has_apic, cpu.has_x2apic
    );
    let _ = writeln!(
        serial,
        "[cpu] NX: {}, 1 GiB pages: {}, SYSCALL: {}, long mode: {}",
        cpu.has_nx, cpu.has_1gib_pages, cpu.has_syscall, cpu.has_long_mode
    );

    // SAFETY: Limine populated the memory-map response before transferring control.
    if let Some(memory_map) = unsafe { boot::limine::memory_map_summary() } {
        let usable_mib = memory_map.usable_bytes / (1024 * 1024);
        let _ = writeln!(
            serial,
            "[memory] {} regions, {} usable regions, {} MiB usable",
            memory_map.entry_count, memory_map.usable_region_count, usable_mib
        );
    } else {
        let _ = writeln!(serial, "[memory] Limine memory map unavailable or invalid");
    }

    // SAFETY: Limine populated the HHDM response before transferring control.
    let direct_map_offset = unsafe { boot::limine::hhdm_offset() };
    if let Some(offset) = direct_map_offset {
        let _ = writeln!(serial, "[memory] HHDM offset: {:#018x}", offset);
    } else {
        let _ = writeln!(serial, "[memory] Limine HHDM response unavailable");
    }

    // SAFETY: Limine's memory map remains available throughout early kernel startup.
    if let Some(mut allocator) = unsafe { memory::allocator::PhysicalFrameAllocator::new() } {
        if let Some(frame) = allocator.allocate_frame() {
            let _ = writeln!(
                serial,
                "[memory] Allocated physical frame at {:#018x}",
                frame.start_address
            );
            if let Some(offset) = direct_map_offset {
                match memory::frame::physical_to_virtual(frame.start_address, offset) {
                    Some(virtual_address) => {
                        let _ = writeln!(
                            serial,
                            "[memory] Direct-map virtual address: {:#018x}",
                            virtual_address.as_u64()
                        );
                    }
                    None => {
                        let _ = writeln!(
                            serial,
                            "[memory] Physical address is not canonical through HHDM"
                        );
                    }
                }
            }
        } else {
            let _ = writeln!(serial, "[memory] No usable physical frames available");
        }
    } else {
        let _ = writeln!(serial, "[memory] Physical frame allocator unavailable");
    }

    #[cfg(feature = "exception-test")]
    {
        let _ = writeln!(serial, "[test] Triggering controlled breakpoint exception");
        // SAFETY: The exception-test feature deliberately invokes vector 3; the
        // installed handler reports the exception and halts without returning.
        unsafe { core::arch::asm!("int3", options(nomem, nostack)) };
    }

    let _ = writeln!(serial, "[boot] Milestone 0.8: framebuffer display");
    let _ = writeln!(serial, "[boot] Display initialisation complete; CPU halted");

    match display::framebuffer::draw_boot_screen() {
        Some((width, height)) => {
            let _ = writeln!(serial, "[display] Framebuffer online: {}x{} RGB", width, height);
        }
        None => {
            let _ = writeln!(serial, "[display] No supported Limine RGB framebuffer");
        }
    }
    halt()
}

#[no_mangle]
pub extern "C" fn kernel_exception_handler(
    vector: u64,
    error_code: u64,
    instruction_pointer: u64,
) -> ! {
    let mut serial = SerialPort;
    let _ = writeln!(
        serial,
        "\n[CPU EXCEPTION] {} (vector {})",
        exception_name(vector),
        vector
    );
    let _ = writeln!(serial, "[exception] Error code: {:#018x}", error_code);
    let _ = writeln!(
        serial,
        "[exception] Instruction pointer: {:#018x}",
        instruction_pointer
    );
    halt()
}

fn exception_name(vector: u64) -> &'static str {
    match vector {
        0 => "Divide Error",
        1 => "Debug",
        2 => "Non-Maskable Interrupt",
        3 => "Breakpoint",
        4 => "Overflow",
        5 => "Bound Range Exceeded",
        6 => "Invalid Opcode",
        7 => "Device Not Available",
        8 => "Double Fault",
        9 => "Coprocessor Segment Overrun",
        10 => "Invalid TSS",
        11 => "Segment Not Present",
        12 => "Stack-Segment Fault",
        13 => "General Protection Fault",
        14 => "Page Fault",
        15 => "Reserved",
        16 => "x87 Floating-Point Exception",
        17 => "Alignment Check",
        18 => "Machine Check",
        19 => "SIMD Floating-Point Exception",
        20 => "Virtualisation Exception",
        21 => "Control Protection Exception",
        22..=28 => "Reserved",
        29 => "VMM Communication Exception",
        30 => "Security Exception",
        31 => "Reserved",
        _ => "Unknown Exception",
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut serial = SerialPort;
    let _ = writeln!(serial, "\n[KERNEL PANIC] {}", info);
    halt()
}

fn halt() -> ! {
    loop {
        // SAFETY: Halt the CPU; interrupts remain disabled until an IDT is installed.
        unsafe {
            core::arch::asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}
