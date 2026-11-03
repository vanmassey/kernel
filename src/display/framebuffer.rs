use crate::boot::limine::{self, FramebufferInfo};

const RGB_MEMORY_MODEL: u8 = 1;
const CHARACTER_WIDTH: u64 = 8;
const CHARACTER_HEIGHT: u64 = 16;
const BLACK: Colour = Colour(0, 0, 0);
const LIGHT_GREY: Colour = Colour(192, 192, 192);

#[derive(Clone, Copy)]
struct Colour(u8, u8, u8);

struct Display {
    info: FramebufferInfo,
    cursor_x: u64,
    cursor_y: u64,
    escape_state: u8,
}

static mut DISPLAY: Option<Display> = None;

impl Display {
    fn new(info: FramebufferInfo) -> Option<Self> {
        if info.address.is_null()
            || info.width < 320
            || info.height < 200
            || info.width > 8192
            || info.height > 8192
            || info.bpp != 32
            || info.memory_model != RGB_MEMORY_MODEL
            || info.pitch < info.width.checked_mul(4)?
            || info.pitch > 65_536
            || info.pitch % 4 != 0
            || !valid_mask(info.red_mask_size, info.red_mask_shift)
            || !valid_mask(info.green_mask_size, info.green_mask_shift)
            || !valid_mask(info.blue_mask_size, info.blue_mask_shift)
        {
            return None;
        }

        Some(Self {
            info,
            cursor_x: 0,
            cursor_y: 0,
            escape_state: 0,
        })
    }

    fn colour_value(&self, colour: Colour) -> u32 {
        channel(colour.0, self.info.red_mask_size, self.info.red_mask_shift)
            | channel(
                colour.1,
                self.info.green_mask_size,
                self.info.green_mask_shift,
            )
            | channel(
                colour.2,
                self.info.blue_mask_size,
                self.info.blue_mask_shift,
            )
    }

    fn pixel(&mut self, x: u64, y: u64, colour: Colour) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let Some(row) = y.checked_mul(self.info.pitch) else {
            return;
        };
        let Some(column) = x.checked_mul(4) else {
            return;
        };
        let Some(offset) = row.checked_add(column) else {
            return;
        };

        // SAFETY: Coordinates are bounded by the validated Limine framebuffer.
        unsafe {
            core::ptr::write_volatile(
                self.info.address.add(offset as usize).cast::<u32>(),
                self.colour_value(colour),
            );
        }
    }

    fn rectangle(&mut self, x: u64, y: u64, width: u64, height: u64, colour: Colour) {
        let right = x.saturating_add(width).min(self.info.width);
        let bottom = y.saturating_add(height).min(self.info.height);

        for py in y.min(self.info.height)..bottom {
            for px in x.min(self.info.width)..right {
                self.pixel(px, py, colour);
            }
        }
    }

    fn draw_character(&mut self, x: u64, y: u64, character: char, colour: Colour) {
        let code = character as usize;
        let glyph = if (32..=126).contains(&code) {
            &super::vga_font::FONT[code - 32]
        } else {
            &super::vga_font::FONT[0]
        };

        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..8 {
                if bits & (1 << column) != 0 {
                    self.pixel(x + column as u64, y + row as u64, colour);
                }
            }
        }
    }

    fn cell_width(&self) -> u64 {
        CHARACTER_WIDTH
    }

    fn cell_height(&self) -> u64 {
        CHARACTER_HEIGHT
    }

    fn clear(&mut self) {
        self.rectangle(0, 0, self.info.width, self.info.height, BLACK);
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y = self.cursor_y.saturating_add(self.cell_height());

        if self.cursor_y + self.cell_height() > self.info.height {
            self.scroll();
        }
    }

    fn scroll(&mut self) {
        let amount = self.cell_height().min(self.info.height);
        let bytes = (self.info.height - amount).saturating_mul(self.info.pitch) as usize;

        // SAFETY: Both ranges are inside the framebuffer; ptr::copy permits overlap.
        unsafe {
            core::ptr::copy(
                self.info.address.add((amount * self.info.pitch) as usize),
                self.info.address,
                bytes,
            );
        }

        self.rectangle(0, self.info.height - amount, self.info.width, amount, BLACK);
        self.cursor_y = self.info.height.saturating_sub(amount);
    }

    fn put_byte(&mut self, byte: u8) {
        if self.escape_state != 0 {
            self.escape_state = match (self.escape_state, byte) {
                (1, b'[') => 2,
                (2, b'2') => 3,
                (2, b'H') => {
                    self.cursor_x = 0;
                    self.cursor_y = 0;
                    0
                }
                (3, b'J') => {
                    self.clear();
                    0
                }
                _ => 0,
            };
            return;
        }

        match byte {
            0x1b => self.escape_state = 1,
            b'\r' => self.cursor_x = 0,
            b'\n' => self.newline(),
            8 | 127 => {
                if self.cursor_x >= self.cell_width() {
                    self.cursor_x -= self.cell_width();
                    self.rectangle(
                        self.cursor_x,
                        self.cursor_y,
                        self.cell_width(),
                        self.cell_height(),
                        BLACK,
                    );
                }
            }
            b'\t' => {
                for _ in 0..4 {
                    self.put_byte(b' ');
                }
            }
            0x20..=0x7e => {
                self.draw_character(self.cursor_x, self.cursor_y, byte as char, LIGHT_GREY);
                self.cursor_x = self.cursor_x.saturating_add(self.cell_width());

                if self.cursor_x + self.cell_width() > self.info.width {
                    self.newline();
                }
            }
            _ => {}
        }
    }

    fn draw_boot_screen(&mut self) {
        // Begin with a clean text console; boot diagnostics remain on serial.
        self.clear();
    }
}

/// Writes one byte to the active framebuffer text console.
pub fn write_byte(byte: u8) {
    // SAFETY: The display is installed before the single-core shell starts.
    unsafe {
        let slot = core::ptr::addr_of_mut!(DISPLAY);
        if let Some(slot) = slot.as_mut() {
            if let Some(display) = slot.as_mut() {
                display.put_byte(byte);
            }
        }
    }
}

/// Draws the DOS-style Sestina console using the framebuffer supplied by Limine.
pub fn draw_boot_screen() -> Option<(u64, u64)> {
    // SAFETY: Limine populates the framebuffer response before kernel entry.
    let info = unsafe { limine::framebuffer()? };
    let mut display = Display::new(info)?;
    display.draw_boot_screen();

    // SAFETY: Installed before the shell starts; no concurrent display access exists yet.
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(DISPLAY), Some(display));
    }

    Some((info.width, info.height))
}

fn valid_mask(size: u8, shift: u8) -> bool {
    size > 0 && size <= 8 && shift < 32 && u16::from(size) + u16::from(shift) <= 32
}

fn channel(value: u8, size: u8, shift: u8) -> u32 {
    let maximum = (1_u32 << size) - 1;
    ((u32::from(value) * maximum / 255) & maximum) << shift
}
