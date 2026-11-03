//! Minimal serial command-line interface for early Sestina development.

use core::fmt::{self, Write};

use crate::arch::x86_64::cpu::CpuInfo;
use crate::arch::x86_64::interrupts;

struct Console;

impl Console {
    const COM1: u16 = 0x3f8;

    fn output(port: u16, value: u8) {
        // SAFETY: COM1 is the kernel's configured serial console.
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") port,
                in("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }

    fn input(port: u16) -> u8 {
        let value: u8;
        // SAFETY: Read the selected COM1 UART register.
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

    fn put_byte(byte: u8) {
        for _ in 0..1_000_000 {
            if Self::input(Self::COM1 + 5) & 0x20 != 0 {
                Self::output(Self::COM1, byte);
                return;
            }
            core::hint::spin_loop();
        }
    }

    fn get_byte() -> u8 {
        loop {
            if Self::input(Self::COM1 + 5) & 0x01 != 0 {
                return Self::input(Self::COM1);
            }
            core::hint::spin_loop();
        }
    }
}

impl Write for Console {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                Console::put_byte(b'\r');
            }
            Console::put_byte(byte);
        }
        Ok(())
    }
}

/// Runs the first interactive shell over the COM1 serial port.
pub fn run(cpu: &CpuInfo, start_ticks: u64) -> ! {
    let mut console = Console;
    let vendor = core::str::from_utf8(&cpu.vendor).unwrap_or("unknown");
    let mut line = [0u8; 128];

    let _ = writeln!(console, "");
    let _ = writeln!(console, "Sestina operating system");
    let _ = writeln!(console, "Type 'help' to list available commands.");

    loop {
        let _ = write!(console, "sestina> ");
        let mut length = 0usize;

        loop {
            let byte = Console::get_byte();

            match byte {
                b'\r' | b'\n' => {
                    let _ = writeln!(console, "");
                    break;
                }
                8 | 127 => {
                    if length > 0 {
                        length -= 1;
                        let _ = write!(console, "\x08 \x08");
                    }
                }
                0x20..=0x7e if length < line.len() - 1 => {
                    line[length] = byte;
                    length += 1;
                    Console::put_byte(byte);
                }
                _ => {}
            }
        }

        let command = match core::str::from_utf8(&line[..length]) {
            Ok(value) => value.trim(),
            Err(_) => {
                let _ = writeln!(console, "Input error: command is not valid ASCII.");
                continue;
            }
        };

        if command.is_empty() {
            continue;
        }

        match command {
            "help" => {
                let _ = writeln!(console, "Available commands:");
                let _ = writeln!(console, "  help       Show this command list");
                let _ = writeln!(console, "  about      Describe Sestina");
                let _ = writeln!(console, "  uname      Show kernel and architecture");
                let _ = writeln!(console, "  version    Show kernel version");
                let _ = writeln!(console, "  cpu        Show processor information");
                let _ = writeln!(console, "  uptime     Show time since shell startup");
                let _ = writeln!(console, "  ticks      Show hardware timer ticks");
                let _ = writeln!(console, "  clear      Clear a compatible serial terminal");
                let _ = writeln!(console, "  echo TEXT  Print TEXT");
            }
            "about" => {
                let _ = writeln!(console, "Sestina is an experimental x86_64 operating-system kernel.");
                let _ = writeln!(console, "It is written in Rust and boots through Limine.");
            }
            "uname" => {
                let _ = writeln!(console, "Sestina sestina 0.1.0 x86_64");
            }
            "version" => {
                let _ = writeln!(console, "Sestina kernel 0.1.0 (development build)");
            }
            "cpu" => {
                let _ = writeln!(console, "Vendor: {}", vendor);
                let _ = writeln!(console, "SSE2: {}, APIC: {}, x2APIC: {}", cpu.has_sse2, cpu.has_apic, cpu.has_x2apic);
                let _ = writeln!(console, "NX: {}, 1 GiB pages: {}", cpu.has_nx, cpu.has_1gib_pages);
                let _ = writeln!(console, "SYSCALL: {}, long mode: {}", cpu.has_syscall, cpu.has_long_mode);
            }
            "uptime" => {
                let elapsed = interrupts::ticks().wrapping_sub(start_ticks) / 100;
                let _ = writeln!(console, "Uptime: {} seconds", elapsed);
            }
            "ticks" => {
                let _ = writeln!(console, "Timer ticks: {}", interrupts::ticks());
            }
            "clear" => {
                let _ = write!(console, "\x1b[2J\x1b[H");
            }
            _ if command == "echo" => {
                let _ = writeln!(console, "");
            }
            _ if command.starts_with("echo ") => {
                let _ = writeln!(console, "{}", &command[5..]);
            }
            _ => {
                let _ = writeln!(console, "Unknown command: {}", command);
                let _ = writeln!(console, "Type 'help' for a list of commands.");
            }
        }
    }
}
