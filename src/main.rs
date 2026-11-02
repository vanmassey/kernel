#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::vec::Vec;
use core::alloc::Layout;
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

        if let Some(offset) = direct_map_offset {
            let heap_frames = memory::heap::HEAP_SIZE / 4096;
            if let Some(heap_frame) = allocator.allocate_contiguous_frames(heap_frames) {
                let last_physical = heap_frame
                    .start_address
                    .checked_add(memory::heap::HEAP_SIZE as u64 - 1);
                let heap_virtual =
                    memory::frame::physical_to_virtual(heap_frame.start_address, offset);
                let last_virtual = last_physical
                    .and_then(|address| memory::frame::physical_to_virtual(address, offset));

                if let (Some(heap_virtual), Some(_)) = (heap_virtual, last_virtual) {
                    let heap_pointer = heap_virtual.as_u64() as *mut u8;
                    // SAFETY: The allocator reserved this contiguous run from Limine's
                    // usable map, and both ends were validated through the active HHDM.
                    unsafe { core::ptr::write_bytes(heap_pointer, 0, memory::heap::HEAP_SIZE) };

                    if memory::heap::initialise(heap_pointer, memory::heap::HEAP_SIZE) {
                        let _ = writeln!(
                            serial,
                            "[memory] Kernel heap ready: {} KiB at {:#018x}",
                            memory::heap::HEAP_SIZE / 1024,
                            heap_virtual.as_u64()
                        );
                        let _ = writeln!(
                            serial,
                            "[memory] Heap allocator self-test: {}",
                            if memory::heap::self_test() {
                                "passed"
                            } else {
                                "FAILED"
                            }
                        );
                        let _ = writeln!(
                            serial,
                            "[memory] Rust Vec self-test: {}",
                            if heap_vec_self_test() {
                                "passed"
                            } else {
                                "FAILED"
                            }
                        );
                    } else {
                        let _ = writeln!(serial, "[memory] Kernel heap initialisation failed");
                    }
                } else {
                    let _ = writeln!(
                        serial,
                        "[memory] Heap region is not addressable through HHDM"
                    );
                }
            } else {
                let _ = writeln!(
                    serial,
                    "[memory] Could not reserve contiguous frames for heap"
                );
            }
        } else {
            let _ = writeln!(serial, "[memory] Heap unavailable without HHDM");
        }

        if let Some(offset) = direct_map_offset {
            // SAFETY: Early startup is single-core, ring 0, and Limine's HHDM is active.
            match unsafe { memory::paging::create_kernel_address_space(&mut allocator, offset) } {
                Some(address_space) => {
                    let _ = writeln!(
                        serial,
                        "[memory] New PML4 active: {:#018x}",
                        address_space.root_physical_address
                    );
                    let _ = writeln!(
                        serial,
                        "[memory] 4 KiB page mapping self-test: passed ({:#018x} -> {:#018x})",
                        address_space.test_virtual_address, address_space.test_physical_address
                    );
                    let second_virtual = address_space.test_virtual_address + 1024 * 1024 * 1024;
                    let _ = writeln!(
                        serial,
                        "[memory] General 4 KiB mapper self-test: {}",
                        if additional_page_mapping_self_test(&mut allocator, offset, second_virtual)
                        {
                            "passed"
                        } else {
                            "FAILED"
                        }
                    );
                }
                None => {
                    let _ = writeln!(serial, "[memory] Kernel address-space creation failed");
                }
            }
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

    if let Some(offset) = direct_map_offset {
        let kernel_address = _start as usize as u64;
        // SAFETY: The kernel runs in ring 0 with Limine's HHDM and active four-level tables.
        match unsafe { memory::paging::translate_current(kernel_address, offset) } {
            Some(translation) => {
                let _ = writeln!(
                    serial,
                    "[memory] Active page-table translation: {:#018x} -> {:#018x} ({:?})",
                    kernel_address, translation.physical_address, translation.page_size
                );
            }
            None => {
                let _ = writeln!(serial, "[memory] Active page-table translation unavailable");
            }
        }
    }

    match display::framebuffer::draw_boot_screen() {
        Some((width, height)) => {
            let _ = writeln!(
                serial,
                "[display] Framebuffer online: {}x{} RGB",
                width, height
            );
        }
        None => {
            let _ = writeln!(serial, "[display] No supported Limine RGB framebuffer");
        }
    }
    let _ = writeln!(
        serial,
        "[boot] Milestone 0.11: kernel address-space management"
    );
    let _ = writeln!(serial, "[boot] Display initialisation complete; CPU halted");
    halt()
}

fn additional_page_mapping_self_test(
    allocator: &mut memory::allocator::PhysicalFrameAllocator,
    hhdm_offset: u64,
    virtual_address: u64,
) -> bool {
    let Some(frame) = allocator.allocate_frame() else {
        return false;
    };
    let Some(last_physical) = frame.start_address.checked_add(4095) else {
        return false;
    };
    let Some(direct_address) = memory::frame::physical_to_virtual(frame.start_address, hhdm_offset)
    else {
        return false;
    };
    if memory::frame::physical_to_virtual(last_physical, hhdm_offset).is_none() {
        return false;
    }

    // SAFETY: The frame is newly reserved and its complete page is addressable via the HHDM.
    unsafe {
        core::ptr::write_bytes(direct_address.as_u64() as *mut u8, 0, 4096);
    }
    // SAFETY: Early boot is single-core, ring 0, and the supplied allocator exclusively
    // owns its frames; the active kernel page tables and HHDM are valid.
    if unsafe {
        memory::paging::map_4k_current(allocator, hhdm_offset, virtual_address, frame.start_address)
    }
    .is_err()
    {
        return false;
    }

    const PATTERN: u64 = 0x5345_5354_494e_4102;
    // SAFETY: The mapper has just installed this writable virtual-to-physical mapping.
    unsafe {
        core::ptr::write_volatile(virtual_address as *mut u64, PATTERN);
    }
    // SAFETY: The virtual address and HHDM address alias the same reserved physical frame.
    let mapped = unsafe { core::ptr::read_volatile(virtual_address as *const u64) };
    // SAFETY: The direct-map address was validated above and refers to the same frame.
    let direct = unsafe { core::ptr::read_volatile(direct_address.as_u64() as *const u64) };
    mapped == PATTERN && direct == PATTERN
}

fn heap_vec_self_test() -> bool {
    let mut values = Vec::with_capacity(32);
    for value in 0_usize..32 {
        values.push(value * 3);
    }

    values.len() == 32
        && values
            .iter()
            .enumerate()
            .all(|(index, value)| *value == index * 3)
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

#[alloc_error_handler]
fn allocation_error(layout: Layout) -> ! {
    let mut serial = SerialPort;
    let _ = writeln!(
        serial,
        "\n[ALLOC ERROR] Could not allocate {} bytes with alignment {}",
        layout.size(),
        layout.align()
    );
    halt()
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
