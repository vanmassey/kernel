use core::arch::global_asm;
use core::mem::size_of;
use core::ptr;

const IDT_ENTRIES: usize = 256;
const CPU_EXCEPTION_COUNT: usize = 32;
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
            options: u16::from(INTERRUPT_GATE) << 8,
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
    ".macro ISR_NOERR num",
    ".global x86_exception_stub_\\num",
    ".type x86_exception_stub_\\num, @function",
    "x86_exception_stub_\\num:",
    "push 0",
    "push \\num",
    "jmp x86_exception_common",
    ".endm",
    ".macro ISR_ERR num",
    ".global x86_exception_stub_\\num",
    ".type x86_exception_stub_\\num, @function",
    "x86_exception_stub_\\num:",
    "push \\num",
    "jmp x86_exception_common",
    ".endm",
    "ISR_NOERR 0",
    "ISR_NOERR 1",
    "ISR_NOERR 2",
    "ISR_NOERR 3",
    "ISR_NOERR 4",
    "ISR_NOERR 5",
    "ISR_NOERR 6",
    "ISR_NOERR 7",
    "ISR_ERR 8",
    "ISR_NOERR 9",
    "ISR_ERR 10",
    "ISR_ERR 11",
    "ISR_ERR 12",
    "ISR_ERR 13",
    "ISR_ERR 14",
    "ISR_NOERR 15",
    "ISR_NOERR 16",
    "ISR_ERR 17",
    "ISR_NOERR 18",
    "ISR_NOERR 19",
    "ISR_NOERR 20",
    "ISR_ERR 21",
    "ISR_NOERR 22",
    "ISR_NOERR 23",
    "ISR_NOERR 24",
    "ISR_NOERR 25",
    "ISR_NOERR 26",
    "ISR_NOERR 27",
    "ISR_NOERR 28",
    "ISR_ERR 29",
    "ISR_ERR 30",
    "ISR_NOERR 31",
    ".global x86_exception_halt_stub",
    ".type x86_exception_halt_stub, @function",
    "x86_exception_halt_stub:",
    "cli",
    "2:",
    "hlt",
    "jmp 2b",
    "x86_exception_common:",
    "cld",
    "mov rdi, [rsp]",
    "mov rsi, [rsp + 8]",
    "mov rdx, [rsp + 16]",
    "and rsp, -16",
    "call kernel_exception_handler",
    "1:",
    "cli",
    "hlt",
    "jmp 1b",
    ".global x86_timer_irq_stub",
    ".type x86_timer_irq_stub, @function",
    "x86_timer_irq_stub:",
    "cld",
    "push rax", "push rbx", "push rcx", "push rdx", "push rbp", "push rsi", "push rdi",
    "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
    "mov r13, rsp",
    "sub rsp, 528",
    "and rsp, -16",
    "fxsave64 [rsp]",
    "mov r12, rsp",
    "call kernel_timer_interrupt",
    "fxrstor64 [r12]",
    "mov rsp, r13",
    "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
    "pop rdi", "pop rsi", "pop rbp", "pop rdx", "pop rcx", "pop rbx", "pop rax",
    "iretq",
    ".section .rodata",
    ".align 8",
    ".global x86_exception_stub_table",
    "x86_exception_stub_table:",
    ".quad x86_exception_stub_0, x86_exception_stub_1, x86_exception_stub_2, x86_exception_stub_3",
    ".quad x86_exception_stub_4, x86_exception_stub_5, x86_exception_stub_6, x86_exception_stub_7",
    ".quad x86_exception_stub_8, x86_exception_stub_9, x86_exception_stub_10, x86_exception_stub_11",
    ".quad x86_exception_stub_12, x86_exception_stub_13, x86_exception_stub_14, x86_exception_stub_15",
    ".quad x86_exception_stub_16, x86_exception_stub_17, x86_exception_stub_18, x86_exception_stub_19",
    ".quad x86_exception_stub_20, x86_exception_stub_21, x86_exception_stub_22, x86_exception_stub_23",
    ".quad x86_exception_stub_24, x86_exception_stub_25, x86_exception_stub_26, x86_exception_stub_27",
    ".quad x86_exception_stub_28, x86_exception_stub_29, x86_exception_stub_30, x86_exception_stub_31",
);

extern "C" {
    static x86_exception_stub_table: [u64; CPU_EXCEPTION_COUNT];
    fn x86_exception_halt_stub();
    fn x86_timer_irq_stub();
}

/// Installs an IDT with dedicated diagnostic stubs for CPU exceptions 0–31.
/// Other vectors retain the non-returning emergency halt handler.
///
/// # Safety
/// Call during single-core early boot with maskable interrupts disabled. The
/// kernel must remain in the same code segment while this IDT is active.
pub unsafe fn initialize() {
    let emergency_handler = x86_exception_halt_stub as *const () as usize as u64;
    let timer_handler = x86_timer_irq_stub as *const () as usize as u64;
    let selector: u16;

    // SAFETY: Reading CS obtains the active code-segment selector for IDT gates.
    unsafe {
        core::arch::asm!("mov {0:x}, cs", out(reg) selector, options(nomem, nostack, preserves_flags));
    }

    // SAFETY: Early boot is single-threaded and interrupts are disabled.
    let idt_ptr = ptr::addr_of_mut!(IDT).cast::<IdtEntry>();
    for index in 0..IDT_ENTRIES {
        // SAFETY: index is bounded by the 256-entry IDT allocation.
        unsafe {
            ptr::write(
                idt_ptr.add(index),
                IdtEntry::interrupt_gate(emergency_handler, selector),
            );
        }
    }

    // SAFETY: The assembly table contains exactly 32 linked exception-stub addresses.
    let exception_table = ptr::addr_of!(x86_exception_stub_table);
    for index in 0..CPU_EXCEPTION_COUNT {
        // SAFETY: index is bounded by the 32-entry assembly table.
        let handler = unsafe { ptr::read(exception_table.cast::<u64>().add(index)) };
        // SAFETY: index is bounded by the first 32 entries of the IDT.
        unsafe {
            ptr::write(
                idt_ptr.add(index),
                IdtEntry::interrupt_gate(handler, selector),
            );
        }
    }

    // IRQ0 is delivered on vector 32 after the legacy PIC is remapped.
    // SAFETY: The assembly entry preserves general registers and returns with IRETQ.
    unsafe {
        ptr::write(
            idt_ptr.add(32),
            IdtEntry::interrupt_gate(timer_handler, selector),
        );
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
