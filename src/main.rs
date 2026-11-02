#![no_std]
#![no_main]

use core::fmt::{self, Write};
use core::panic::PanicInfo;

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
        unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
    }

    unsafe fn input(port: u16) -> u8 {
        let value: u8;
        // SAFETY: Reading the selected UART register is intentional during early boot.
        unsafe { core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags)); }
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
            if byte == b'\n' { self.write_byte(b'\r'); }
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
    let _ = writeln!(serial, "  KERNEL :: x86_64 Rust kernel");
    let _ = writeln!(serial, "========================================");
    let _ = writeln!(serial, "[boot] Entered kernel entry point");
    let _ = writeln!(serial, "[boot] COM1 serial console initialised");
    let _ = writeln!(serial, "[boot] Limine base revision request present");
    let _ = writeln!(serial, "[boot] Milestone 0.1 foundation reached");
    let _ = writeln!(serial, "[boot] CPU halted; next: memory and interrupts");
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
        unsafe { core::arch::asm!("cli", "hlt", options(nomem, nostack)); }
    }
}
