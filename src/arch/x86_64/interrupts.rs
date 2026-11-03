//! Legacy 8259 PIC and PIT timer support for early x86_64 bring-up.

use core::sync::atomic::{AtomicU64, Ordering};

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xa0;
const PIC2_DATA: u16 = 0xa1;
const PIT_COMMAND: u16 = 0x43;
const PIT_CHANNEL_0: u16 = 0x40;
const PIT_INPUT_HZ: u32 = 1_193_182;
const PIT_FREQUENCY_HZ: u32 = 100;
const PIC_EOI: u8 = 0x20;

static TICKS: AtomicU64 = AtomicU64::new(0);

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: The caller must execute in ring 0 and select a valid legacy I/O port.
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

unsafe fn io_wait() {
    // SAFETY: Port 0x80 is traditionally used for a short delay during PIC programming.
    unsafe { outb(0x80, 0) };
}

/// Remaps the legacy PIC and programs PIT channel 0 for a 100 Hz IRQ0.
///
/// Only IRQ0 is unmasked. All other legacy PIC IRQs remain masked until their
/// handlers and device drivers are implemented. This function leaves IF clear.
///
/// # Safety
/// Must run in ring 0 after the IDT has been installed, with interrupts disabled,
/// before any code unmasks or enables legacy hardware interrupts.
pub unsafe fn initialize_legacy_pic_and_pit() {
    // ICW1: begin initialization and expect ICW4.
    // SAFETY: The caller guarantees privileged early-boot execution with IF clear.
    unsafe {
        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();

        // ICW2: move the PIC vectors away from CPU exception vectors.
        outb(PIC1_DATA, 0x20);
        io_wait();
        outb(PIC2_DATA, 0x28);
        io_wait();

        // ICW3: the slave is connected to master IRQ2.
        outb(PIC1_DATA, 0x04);
        io_wait();
        outb(PIC2_DATA, 0x02);
        io_wait();

        // ICW4: 8086/88 mode.
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        // Unmask only the master timer IRQ. Keep every other IRQ masked.
        outb(PIC1_DATA, 0xfe);
        outb(PIC2_DATA, 0xff);

        // PIT mode 3 (square wave), channel 0, low byte then high byte.
        let divisor = (PIT_INPUT_HZ / PIT_FREQUENCY_HZ) as u16;
        outb(PIT_COMMAND, 0x36);
        outb(PIT_CHANNEL_0, divisor as u8);
        outb(PIT_CHANNEL_0, (divisor >> 8) as u8);
    }

    TICKS.store(0, Ordering::Release);
}

/// Returns the number of timer interrupts observed since initialisation.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Acquire)
}

/// IRQ0 entry point called by the assembly interrupt stub.
///
/// Keep this handler deliberately small: it only updates the monotonic tick
/// counter and acknowledges the master PIC. It does not allocate or log.
#[no_mangle]
pub extern "C" fn kernel_timer_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: This handler is entered from ring 0 through the installed IRQ0 gate.
    unsafe { outb(PIC1_COMMAND, PIC_EOI) };
}
