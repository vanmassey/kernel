//! Minimal serial command-line interface for early Sestina development.

use core::fmt::{self, Write};

use crate::arch::x86_64::cpu::CpuInfo;
use crate::filesystem::{ramfs::RamFs, vfs::{FileSystem, NodeKind}};
use crate::arch::x86_64::interrupts;
use crate::arch::x86_64::keyboard::Keyboard;

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

    fn get_byte(keyboard: &mut Keyboard) -> u8 {
        loop {
            if let Some(event) = keyboard.poll_event() {
                if event.pressed && !event.extended {
                    match event.scancode {
                        0x1c => return b'\r',
                        0x0e => return 8,
                        _ => {
                            if let Some(byte) = event.ascii {
                                return byte;
                            }
                        }
                    }
                }
            }

            if let Some(byte) = Self::try_get_byte() {
                return byte;
            }

            // SAFETY: The shell enables maskable interrupts before entering this loop.
            // The PIT tick wakes HLT so keyboard and serial input can be polled again.
            unsafe {
                core::arch::asm!("hlt", options(nomem, nostack));
            }
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
    let mut keyboard = Keyboard::new();
    // RAMFS is session-local until a block-storage driver is available.
    let mut filesystem = RamFs::new();
    let vendor = core::str::from_utf8(&cpu.vendor).unwrap_or("unknown");
    let mut line = [0u8; LINE_CAPACITY];
    let mut history_entries = [[0u8; LINE_CAPACITY]; HISTORY_CAPACITY];
    let mut history_lengths = [0usize; HISTORY_CAPACITY];
    let mut history_next = 0usize;
    let mut history_count = 0usize;

    // SAFETY: The IDT, PIC, and PIT are initialised before the shell is entered.
    // Enable IRQ0 so uptime and timer ticks continue while the shell is running.
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }

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
            let byte = Console::get_byte(&mut keyboard);

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
                                line[..length].copy_from_slice(&history_entries[index][..length]);
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

        let mut parts = command.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("");
        let args = parts.next().unwrap_or("").trim();

        match name {
            "help" | "commands" => print_help(&mut console, args),
            "about" => {
                let _ = writeln!(
                    console,
                    "Sestina is an experimental x86_64 operating-system kernel."
                );
                let _ = writeln!(console, "It is written in Rust and boots through Limine.");
            }
            "hostname" => {
                let _ = writeln!(console, "sestina");
            }
            "uname" => {
                let _ = writeln!(
                    console,
                    "Sestina sestina {} x86_64",
                    env!("CARGO_PKG_VERSION")
                );
            }
            "version" | "ver" => {
                let _ = writeln!(
                    console,
                    "Sestina kernel {} (development build)",
                    env!("CARGO_PKG_VERSION")
                );
            }
            "sysinfo" | "status" => {
                let _ = writeln!(console, "System: Sestina x86_64");
                let _ = writeln!(console, "Kernel: {}", env!("CARGO_PKG_VERSION"));
                let _ = writeln!(console, "CPU: {}", vendor);
                let _ = writeln!(console, "Timer: 100 Hz, {} ticks", interrupts::ticks());
                print_uptime(&mut console, start_ticks);
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
            "uptime" => print_uptime(&mut console, start_ticks),
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
            "pwd" => {
                let _ = writeln!(console, "/");
            }
            "ls" => {
                let path = if args.is_empty() { "/" } else { args };
                match filesystem.list_dir(path) {
                    Ok(entries) => {
                        for entry in entries {
                            match entry.kind {
                                NodeKind::Directory => {
                                    let _ = writeln!(console, "{}/", entry.name);
                                }
                                NodeKind::File => {
                                    let _ = writeln!(console, "{}", entry.name);
                                }
                            }
                        }
                    }
                    Err(error) => print_fs_error(&mut console, error),
                }
            }
            "mkdir" => {
                if args.is_empty() {
                    let _ = writeln!(console, "Usage: mkdir /PATH");
                } else if let Err(error) = filesystem.create_dir(args) {
                    print_fs_error(&mut console, error);
                }
            }
            "touch" => {
                if args.is_empty() {
                    let _ = writeln!(console, "Usage: touch /PATH");
                } else if let Err(error) = filesystem.create_file(args) {
                    print_fs_error(&mut console, error);
                }
            }
            "cat" => {
                if args.is_empty() {
                    let _ = writeln!(console, "Usage: cat /PATH");
                } else {
                    match filesystem.read_file(args) {
                        Ok(contents) => match core::str::from_utf8(contents) {
                            Ok(text) => {
                                let _ = writeln!(console, "{}", text);
                            }
                            Err(_) => {
                                let _ = writeln!(console, "File is not valid UTF-8 text.");
                            }
                        },
                        Err(error) => print_fs_error(&mut console, error),
                    }
                }
            }
            "write" => {
                let mut write_args = args.splitn(2, char::is_whitespace);
                let path = write_args.next().unwrap_or("");
                let contents = write_args.next().unwrap_or("").trim_start();
                if path.is_empty() || contents.is_empty() {
                    let _ = writeln!(console, "Usage: write /PATH TEXT");
                } else {
                    match filesystem.write_file(path, contents.as_bytes()) {
                        Ok(()) => {
                            let _ = writeln!(console, "Wrote {} bytes.", contents.len());
                        }
                        Err(error) => print_fs_error(&mut console, error),
                    }
                }
            }
            _ => {
                let _ = writeln!(console, "Unknown command: {}", name);
                let _ = writeln!(console, "Type 'help' for a list of commands.");
            }
        }
    }
}

fn print_fs_error(console: &mut Console, error: crate::filesystem::vfs::FsError) {
    use crate::filesystem::vfs::FsError;

    let message = match error {
        FsError::InvalidPath => "invalid path (use an absolute path; '..' is not supported)",
        FsError::NotFound => "path not found",
        FsError::AlreadyExists => "an entry with that name already exists",
        FsError::NotDirectory => "a path component is not a directory",
        FsError::IsDirectory => "the path is a directory, not a file",
    };
    let _ = writeln!(console, "Filesystem error: {}", message);
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
        let _ = writeln!(console, "  commands        Alias for help");
        let _ = writeln!(console, "  about           Describe Sestina");
        let _ = writeln!(console, "  hostname        Show the system hostname");
        let _ = writeln!(
            console,
            "  uname           Show kernel name, version and architecture"
        );
        let _ = writeln!(console, "  version, ver    Show kernel version");
        let _ = writeln!(console, "  cpu             Show processor information");
        let _ = writeln!(console, "  sysinfo, status Show a system summary");
        let _ = writeln!(console, "  uptime          Show time since shell startup");
        let _ = writeln!(console, "  ticks           Show hardware timer ticks");
        let _ = writeln!(console, "  history         Show the last eight commands");
        let _ = writeln!(
            console,
            "  clear           Clear a compatible serial terminal"
        );
        let _ = writeln!(console, "  echo TEXT       Print TEXT");
        let _ = writeln!(console, "  pwd             Show the current directory (/)");
        let _ = writeln!(console, "  ls [PATH]       List a directory (default: /)");
        let _ = writeln!(console, "  mkdir /PATH     Create a directory");
        let _ = writeln!(console, "  touch /PATH     Create an empty file");
        let _ = writeln!(console, "  cat /PATH       Print a text file");
        let _ = writeln!(console, "  write /PATH TEXT  Replace a file's contents");
        return;
    }

    match topic {
        "help" => {
            let _ = writeln!(
                console,
                "help [COMMAND] - list commands or explain one command"
            );
        }
        "commands" => {
            let _ = writeln!(console, "commands - list the available shell commands");
        }
        "about" => {
            let _ = writeln!(
                console,
                "about - describe the Sestina operating-system project"
            );
        }
        "hostname" => {
            let _ = writeln!(console, "hostname - print the system hostname");
        }
        "sysinfo" | "status" => {
            let _ = writeln!(
                console,
                "sysinfo - show kernel, CPU, timer and uptime information"
            );
        }
        "uname" => {
            let _ = writeln!(console, "uname - print the kernel name and architecture");
        }
        "version" | "ver" => {
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
        "pwd" => {
            let _ = writeln!(console, "pwd - print the current directory (always / for now)");
        }
        "ls" => {
            let _ = writeln!(console, "ls [PATH] - list a directory; defaults to /");
        }
        "mkdir" => {
            let _ = writeln!(console, "mkdir /PATH - create a directory");
        }
        "touch" => {
            let _ = writeln!(console, "touch /PATH - create an empty file");
        }
        "cat" => {
            let _ = writeln!(console, "cat /PATH - print a UTF-8 text file");
        }
        "write" => {
            let _ = writeln!(console, "write /PATH TEXT - replace a file's contents");
        }
        _ => {
            let _ = writeln!(console, "No help available for '{}'.", topic);
        }
    }
}

fn print_uptime(console: &mut Console, start_ticks: u64) {
    let elapsed_ticks = interrupts::ticks().wrapping_sub(start_ticks);
    let total_seconds = elapsed_ticks / 100;
    let days = total_seconds / 86_400;
    let hours = (total_seconds / 3_600) % 24;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        let _ = writeln!(
            console,
            "Uptime: {}d {:02}:{:02}:{:02}",
            days, hours, minutes, seconds
        );
    } else {
        let _ = writeln!(
            console,
            "Uptime: {:02}:{:02}:{:02}",
            hours, minutes, seconds
        );
    }
}
