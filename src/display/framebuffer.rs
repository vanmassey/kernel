use crate::boot::limine::{self, FramebufferInfo};

const RGB_MEMORY_MODEL: u8 = 1;

#[derive(Clone, Copy)]
struct Colour(u8, u8, u8);

struct Display {
    info: FramebufferInfo,
}

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

        Some(Self { info })
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

        // SAFETY: Coordinates are within the Limine framebuffer dimensions and pitch.
        // The bootloader-provided framebuffer mapping remains active during early boot.
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

    fn text(&mut self, x: u64, y: u64, value: &str, scale: u64, colour: Colour) {
        let mut cursor = x;
        for character in value.chars() {
            let rows = glyph(character.to_ascii_uppercase());
            for (row, bits) in rows.iter().enumerate() {
                for column in 0..5 {
                    if bits & (1 << (4 - column)) != 0 {
                        self.rectangle(
                            cursor + column * scale,
                            y + row as u64 * scale,
                            scale,
                            scale,
                            colour,
                        );
                    }
                }
            }
            cursor = cursor.saturating_add(6 * scale);
        }
    }

    fn draw_boot_screen(&mut self) {
        let navy = Colour(13, 24, 38);
        let panel = Colour(22, 37, 54);
        let white = Colour(235, 239, 242);
        let muted = Colour(154, 172, 187);
        let green = Colour(119, 190, 145);
        let accent = Colour(183, 157, 112);
        let scale = if self.info.width >= 1000 { 2 } else { 1 };
        let margin = 32 * scale;

        self.rectangle(0, 0, self.info.width, self.info.height, navy);
        self.rectangle(0, 0, self.info.width, 5 * scale, accent);
        self.text(margin, margin, "SESTINA", 3 * scale, white);
        self.text(
            margin,
            margin + 30 * scale,
            "OPERATING SYSTEM",
            scale,
            muted,
        );

        let panel_y = margin + 76 * scale;
        let panel_height = 150 * scale;
        self.rectangle(
            margin,
            panel_y,
            self.info.width.saturating_sub(margin * 2),
            panel_height,
            panel,
        );

        let line_gap = 21 * scale;
        let status_x = margin + 18 * scale;
        let status_y = panel_y + 17 * scale;
        self.text(status_x, status_y, "KERNEL INITIALISED", scale, white);
        self.text(status_x, status_y + line_gap, "CPU: X86_64", scale, green);
        self.text(
            status_x,
            status_y + line_gap * 2,
            "MEMORY MAP: READY",
            scale,
            green,
        );
        self.text(
            status_x,
            status_y + line_gap * 3,
            "FRAMEBUFFER: ONLINE",
            scale,
            green,
        );

        let footer_y = panel_y + panel_height + 22 * scale;
        self.text(margin, footer_y, "MILESTONE 0.8", scale, accent);
        self.text(
            margin,
            footer_y + 20 * scale,
            "EARLY KERNEL INITIALISATION",
            scale,
            muted,
        );
    }
}

fn valid_mask(size: u8, shift: u8) -> bool {
    size > 0 && size <= 8 && shift < 32 && u16::from(size) + u16::from(shift) <= 32
}

fn channel(value: u8, size: u8, shift: u8) -> u32 {
    let maximum = (1_u32 << size) - 1;
    ((u32::from(value) * maximum / 255) & maximum) << shift
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        ':' => [0, 4, 4, 0, 4, 4, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        ' ' => [0; 7],
        _ => [31, 17, 1, 2, 4, 0, 4],
    }
}

/// Draws the initial Sestina screen using the framebuffer supplied by Limine.
///
/// Returns the display dimensions when a supported RGB framebuffer is available.
pub fn draw_boot_screen() -> Option<(u64, u64)> {
    // SAFETY: Limine's framebuffer response is populated before kernel entry.
    let info = unsafe { limine::framebuffer()? };
    let mut display = Display::new(info)?;
    display.draw_boot_screen();
    Some((info.width, info.height))
}
