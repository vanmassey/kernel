use core::arch::global_asm;
use core::mem::size_of;
use core::ptr;

const IDT_ENTRIES: usize = 256;
const INTERRUPT_GATE: u8 = 0x8e;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    options: u16,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        options: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn interrupt_gate(handler: u64, selector: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            options: (u16::from(INTERRUPT_GATE) << 8),
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::MISSING; IDT_ENTRIES];

global_asm!(
    ".global x86_exception_halt_stub",
    ".type x86_exception_halt_stub, @function",
    "x86_exception_halt_stub:",
    "cli",
    "2:",
    "hlt",
    "jmp 2b",
);

extern "C" {
    fn x86_exception_halt_stub();
}

/// Installs a 256-entry IDT whose gates lead to a non-returning emergency halt.
///
/// This first IDT prevents an unhandled CPU exception from jumping through an
/// invalid descriptor. Per-exception diagnostics and recovery will replace the
/// common emergency handler as the interrupt subsystem develops.
///
/// # Safety
/// Call during single-core early boot with maskable interrupts disabled. The
/// kernel must remain in the same code segment while this IDT is active.
pub unsafe fn initialize() {
    let handler = x86_exception_halt_stub as usize as u64;
    let selector: u16;

    // SAFETY: Reading CS obtains the active code-segment selector for IDT gates.
    unsafe {
        core::arch::asm!("mov {0:x}, cs", out(reg) selector, options(nomem, nostack, preserves_flags));
    }

    // SAFETY: Early boot is single-threaded and interrupts are disabled, so no
    // other execution context can access the table while it is populated.
    let idt_ptr = ptr::addr_of_mut!(IDT).cast::<IdtEntry>();
    for index in 0..IDT_ENTRIES {
        // SAFETY: index is bounded by the 256-entry IDT allocation.
        unsafe {
            ptr::write(
                idt_ptr.add(index),
                IdtEntry::interrupt_gate(handler, selector),
            );
        }
    }

    let descriptor = Idtr {
        limit: (size_of::<IdtEntry>() * IDT_ENTRIES - 1) as u16,
        base: idt_ptr as u64,
    };

    // SAFETY: descriptor references the fully initialised static IDT.
    unsafe {
        core::arch::asm!(
            "lidt [{}]",
            in(reg) &descriptor,
            options(readonly, nostack, preserves_flags)
        );
    }
}
