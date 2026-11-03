//! Polled PS/2 keyboard input using the legacy i8042 data interface.
//!
//! This first-stage driver does not enable IRQ1 or reconfigure the controller.
//! It reads a byte only when the controller reports a pending output byte.

const STATUS_PORT: u16 = 0x64;
const DATA_PORT: u16 = 0x60;
const OUTPUT_BUFFER_FULL: u8 = 1 << 0;
const AUXILIARY_DATA: u8 = 1 << 5;
const STATUS_ERROR: u8 = (1 << 6) | (1 << 7);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyEvent {
    /// Set-1 make-code without the release bit.
    pub scancode: u8,
    pub pressed: bool,
    pub extended: bool,
    /// US-layout ASCII character for printable key presses.
    pub ascii: Option<u8>,
}

/// Stateful decoder and non-blocking PS/2 keyboard poller.
pub struct Keyboard {
    extended_prefix: bool,
    pause_bytes_remaining: u8,
    left_shift: bool,
    right_shift: bool,
    caps_lock: bool,
}

impl Keyboard {
    pub const fn new() -> Self {
        Self {
            extended_prefix: false,
            pause_bytes_remaining: 0,
            left_shift: false,
            right_shift: false,
            caps_lock: false,
        }
    }

    /// Read at most one pending keyboard byte and decode it into an event.
    ///
    /// This is non-blocking. Auxiliary-device bytes and controller error bytes
    /// are consumed and discarded rather than being mistaken for key presses.
    pub fn poll_event(&mut self) -> Option<KeyEvent> {
        // SAFETY: These are the standard legacy i8042 status and data ports.
        // The kernel runs in ring 0; reading status first prevents an empty read.
        let status = unsafe { inb(STATUS_PORT) };
        if status & OUTPUT_BUFFER_FULL == 0 {
            return None;
        }

        // SAFETY: The output buffer was reported full by the controller.
        let byte = unsafe { inb(DATA_PORT) };

        if status & AUXILIARY_DATA != 0 || status & STATUS_ERROR != 0 {
            return None;
        }

        self.feed_scancode(byte)
    }

    fn feed_scancode(&mut self, byte: u8) -> Option<KeyEvent> {
        if self.pause_bytes_remaining != 0 {
            self.pause_bytes_remaining -= 1;
            return None;
        }

        if byte == 0xE1 {
            self.extended_prefix = false;
            self.pause_bytes_remaining = 5;
            return None;
        }

        if byte == 0xE0 {
            self.extended_prefix = true;
            return None;
        }

        let extended = core::mem::replace(&mut self.extended_prefix, false);
        let pressed = byte & 0x80 == 0;
        let scancode = byte & 0x7F;

        if !extended {
            match scancode {
                0x2A => self.left_shift = pressed,
                0x36 => self.right_shift = pressed,
                0x3A if pressed => self.caps_lock = !self.caps_lock,
                _ => {}
            }
        }

        let ascii = if pressed && !extended {
            self.ascii_for(scancode)
        } else {
            None
        };

        Some(KeyEvent {
            scancode,
            pressed,
            extended,
            ascii,
        })
    }

    fn ascii_for(&self, scancode: u8) -> Option<u8> {
        let shifted = self.left_shift || self.right_shift;
        let (normal, upper, is_letter) = match scancode {
            0x02 => (b'1', b'!', false),
            0x03 => (b'2', b'@', false),
            0x04 => (b'3', b'#', false),
            0x05 => (b'4', b'$', false),
            0x06 => (b'5', b'%', false),
            0x07 => (b'6', b'^', false),
            0x08 => (b'7', b'&', false),
            0x09 => (b'8', b'*', false),
            0x0A => (b'9', b'(', false),
            0x0B => (b'0', b')', false),
            0x0C => (b'-', b'_', false),
            0x0D => (b'=', b'+', false),
            0x10 => (b'q', b'Q', true),
            0x11 => (b'w', b'W', true),
            0x12 => (b'e', b'E', true),
            0x13 => (b'r', b'R', true),
            0x14 => (b't', b'T', true),
            0x15 => (b'y', b'Y', true),
            0x16 => (b'u', b'U', true),
            0x17 => (b'i', b'I', true),
            0x18 => (b'o', b'O', true),
            0x19 => (b'p', b'P', true),
            0x1A => (b'[', b'{', false),
            0x1B => (b']', b'}', false),
            0x1E => (b'a', b'A', true),
            0x1F => (b's', b'S', true),
            0x20 => (b'd', b'D', true),
            0x21 => (b'f', b'F', true),
            0x22 => (b'g', b'G', true),
            0x23 => (b'h', b'H', true),
            0x24 => (b'j', b'J', true),
            0x25 => (b'k', b'K', true),
            0x26 => (b'l', b'L', true),
            0x27 => (b';', b':', false),
            0x28 => (b'\'', b'"', false),
            0x29 => (b'`', b'~', false),
            0x2B => (b'\\', b'|', false),
            0x2C => (b'z', b'Z', true),
            0x2D => (b'x', b'X', true),
            0x2E => (b'c', b'C', true),
            0x2F => (b'v', b'V', true),
            0x30 => (b'b', b'B', true),
            0x31 => (b'n', b'N', true),
            0x32 => (b'm', b'M', true),
            0x33 => (b',', b'<', false),
            0x34 => (b'.', b'>', false),
            0x35 => (b'/', b'?', false),
            0x39 => (b' ', b' ', false),
            _ => return None,
        };

        if is_letter {
            Some(if self.caps_lock ^ shifted { upper } else { normal })
        } else {
            Some(if shifted { upper } else { normal })
        }
    }

    /// Exercise the decoder without touching hardware.
    pub fn decoder_self_test() -> bool {
        let mut keyboard = Self::new();

        let lower = keyboard.feed_scancode(0x1E);
        let release = keyboard.feed_scancode(0x9E);
        let shift_down = keyboard.feed_scancode(0x2A);
        let upper = keyboard.feed_scancode(0x1E);
        let shift_up = keyboard.feed_scancode(0xAA);
        let caps_down = keyboard.feed_scancode(0x3A);
        let caps_upper = keyboard.feed_scancode(0x1E);

        matches!(lower, Some(KeyEvent { ascii: Some(b'a'), pressed: true, .. }))
            && matches!(release, Some(KeyEvent { ascii: None, pressed: false, .. }))
            && matches!(shift_down, Some(KeyEvent { scancode: 0x2A, .. }))
            && matches!(upper, Some(KeyEvent { ascii: Some(b'A'), pressed: true, .. }))
            && matches!(shift_up, Some(KeyEvent { scancode: 0x2A, pressed: false, .. }))
            && matches!(caps_down, Some(KeyEvent { scancode: 0x3A, pressed: true, .. }))
            && matches!(caps_upper, Some(KeyEvent { ascii: Some(b'A'), pressed: true, .. }))
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: The caller selects a valid legacy I/O port and executes in ring 0.
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
