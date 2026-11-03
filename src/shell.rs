//! Minimal serial command-line interface for early Sestina development.

use core::fmt::{self, Write};

use crate::arch::x86_64::cpu::CpuInfo;
use crate::arch::x86_64::interrupts;

const LINE_CAPACITY: usize = 128;
const HISTORY_CAPACITY: usize = 8;

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
            if let Some(byte) = Self::try_get_byte() {
                return byte;
            }
            core::hint::spin_loop();
        }
    }

    fn try_get_byte() -> Option<u8> {
        for _ in 0..1_000_000 {
            if Self::input(Self::COM1 + 5) & 0x01 != 0 {
                return Some(Self::input(Self::COM1));
            }
            core::hint::spin_loop();
        }
        None
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
    let mut line = [0u8; LINE_CAPACITY];
    let mut history_entries = [[0u8; LINE_CAPACITY]; HISTORY_CAPACITY];
    let mut history_lengths = [0usize; HISTORY_CAPACITY];
    let mut history_next = 0usize;
    let mut history_count = 0usize;

    let _ = writeln!(console, "");
    let _ = writeln!(console, "Sestina operating system");
    let _ = writeln!(console, "Type 'help' to list available commands.");

    loop {
        let _ = write!(console, "sestina> ");
        let mut length = 0usize;
        let mut history_position = history_count;
        let mut draft = [0u8; LINE_CAPACITY];
        let mut draft_length = 0usize;
        let mut navigating_history = false;

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
                0x1b => {
                    if Console::try_get_byte() == Some(b'[') {
                        match Console::try_get_byte() {
                            Some(b'A') if history_count > 0 && history_position > 0 => {
                                if !navigating_history {
                                    draft[..length].copy_from_slice(&line[..length]);
                                    draft_length = length;
                                    navigating_history = true;
                                }
                                history_position -= 1;
                                let first = (history_next + HISTORY_CAPACITY - history_count)
                                    % HISTORY_CAPACITY;
                                let index = (first + history_position) % HISTORY_CAPACITY;
                                let old_length = length;
                                length = history_lengths[index];
                                line[..length]
                                    .copy_from_slice(&history_entries[index][..length]);
                                redraw_line(&mut console, &line, length, old_length);
                            }
                            Some(b'B')
                                if navigating_history && history_position < history_count =>
                            {
                                history_position += 1;
                                let old_length = length;
                                if history_position == history_count {
                                    length = draft_length;
                                    line[..length].copy_from_slice(&draft[..length]);
                                    navigating_history = false;
                                } else {
                                    let first = (history_next + HISTORY_CAPACITY - history_count)
                                        % HISTORY_CAPACITY;
                                    let index = (first + history_position) % HISTORY_CAPACITY;
                                    length = history_lengths[index];
                                    line[..length]
                                        .copy_from_slice(&history_entries[index][..length]);
                                }
                                redraw_line(&mut console, &line, length, old_length);
                            }
                            _ => {}
                        }
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

        // Keep a small, fixed-size history so the shell needs no heap allocation.
        let command_bytes = command.as_bytes();
        history_entries[history_next][..command_bytes.len()].copy_from_slice(command_bytes);
        history_lengths[history_next] = command_bytes.len();
        history_next = (history_next + 1) % HISTORY_CAPACITY;
        if history_count < HISTORY_CAPACITY {
            history_count += 1;
        }

        let (name, args) = command.split_once(' ').unwrap_or((command, ""));
        let args = args.trim();

        match name {
            "help" => print_help(&mut console, args),
            "about" => {
                let _ = writeln!(
                    console,
                    "Sestina is an experimental x86_64 operating-system kernel."
                );
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
                let _ = writeln!(
                    console,
                    "SSE2: {}, APIC: {}, x2APIC: {}",
                    cpu.has_sse2, cpu.has_apic, cpu.has_x2apic
                );
                let _ = writeln!(
                    console,
                    "NX: {}, 1 GiB pages: {}",
                    cpu.has_nx, cpu.has_1gib_pages
                );
                let _ = writeln!(
                    console,
                    "SYSCALL: {}, long mode: {}",
                    cpu.has_syscall, cpu.has_long_mode
                );
            }
            "uptime" => {
                let elapsed = interrupts::ticks().wrapping_sub(start_ticks) / 100;
                let _ = writeln!(console, "Uptime: {} seconds", elapsed);
            }
            "ticks" => {
                let _ = writeln!(console, "Timer ticks: {}", interrupts::ticks());
            }
            "history" => {
                let first = (history_next + HISTORY_CAPACITY - history_count) % HISTORY_CAPACITY;
                for offset in 0..history_count {
                    let index = (first + offset) % HISTORY_CAPACITY;
                    let entry =
                        core::str::from_utf8(&history_entries[index][..history_lengths[index]])
                            .unwrap_or("<invalid history entry>");
                    let _ = writeln!(console, "{:>2}  {}", offset + 1, entry);
                }
            }
            "clear" => {
                let _ = write!(console, "\\x1b[2J\\x1b[H");
            }
            "echo" => {
                let _ = writeln!(console, "{}", args);
            }
            _ => {
                let _ = writeln!(console, "Unknown command: {}", name);
                let _ = writeln!(console, "Type 'help' for a list of commands.");
            }
        }
    }
}

fn redraw_line(
    console: &mut Console,
    line: &[u8; LINE_CAPACITY],
    length: usize,
    old_length: usize,
) {
    let _ = write!(console, "\rsestina> ");
    for byte in &line[..length] {
        Console::put_byte(*byte);
    }
    for _ in length..old_length {
        Console::put_byte(b' ');
    }
    for _ in length..old_length {
        Console::put_byte(8);
    }
}

fn print_help(console: &mut Console, topic: &str) {
    if topic.is_empty() {
        let _ = writeln!(console, "Available commands:");
        let _ = writeln!(console, "  help [COMMAND]  Show command help");
        let _ = writeln!(console, "  about           Describe Sestina");
        let _ = writeln!(console, "  uname           Show kernel and architecture");
        let _ = writeln!(console, "  version         Show kernel version");
        let _ = writeln!(console, "  cpu             Show processor information");
        let _ = writeln!(console, "  uptime          Show time since shell startup");
        let _ = writeln!(console, "  ticks           Show hardware timer ticks");
        let _ = writeln!(console, "  history         Show the last eight commands");
        let _ = writeln!(
            console,
            "  clear           Clear a compatible serial terminal"
        );
        let _ = writeln!(console, "  echo TEXT       Print TEXT");
        return;
    }

    match topic {
        "help" => {
            let _ = writeln!(
                console,
                "help [COMMAND] - list commands or explain one command"
            );
        }
        "about" => {
            let _ = writeln!(
                console,
                "about - describe the Sestina operating-system project"
            );
        }
        "uname" => {
            let _ = writeln!(console, "uname - print the kernel name and architecture");
        }
        "version" => {
            let _ = writeln!(console, "version - print the kernel version");
        }
        "cpu" => {
            let _ = writeln!(console, "cpu - show detected processor capabilities");
        }
        "uptime" => {
            let _ = writeln!(console, "uptime - show elapsed time since shell startup");
        }
        "ticks" => {
            let _ = writeln!(console, "ticks - show the current PIT tick count");
        }
        "history" => {
            let _ = writeln!(console, "history - list the last eight entered commands");
        }
        "clear" => {
            let _ = writeln!(
                console,
                "clear - send ANSI clear-screen and cursor-home codes"
            );
        }
        "echo" => {
            let _ = writeln!(console, "echo TEXT - print TEXT to the console");
        }
        _ => {
            let _ = writeln!(console, "No help available for '{}'.", topic);
        }
    }
}
