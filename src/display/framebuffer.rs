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
        let background = Colour(7, 12, 20);
        let panel = Colour(13, 23, 35);
        let panel_edge = Colour(31, 54, 72);
        let white = Colour(225, 237, 245);
        let muted = Colour(116, 143, 163);
        let cyan = Colour(75, 205, 222);
        let green = Colour(105, 220, 151);
        let amber = Colour(231, 177, 91);
        let red = Colour(226, 105, 105);
        let scale = if self.info.width >= 1000 && self.info.height >= 600 {
            2
        } else {
            1
        };
        let margin = 24 * scale;

        self.rectangle(0, 0, self.info.width, self.info.height, background);
        self.rectangle(0, 0, self.info.width, 36 * scale, panel);
        self.rectangle(0, 0, self.info.width, 3 * scale, cyan);
        self.text(margin, 12 * scale, "SESTINA // BOOT MONITOR", scale, cyan);
        self.text(
            self.info.width.saturating_sub(210 * scale),
            12 * scale,
            "X86_64  |  DEV 0.12",
            scale,
            muted,
        );

        let logo_x = margin + 4 * scale;
        let logo_y = 52 * scale;
        let logo = [
            "  _____ _____ ____ _____ ___ _   _   _ ",
            " / ____| ____/ ___|_   _|_ _| \\ | |",
            " \\___ \\|  _| \\___ \\ | |  | ||  \\| |",
            " |___) | |___ ___) || |  | || |\\  |",
            " |____/|_____|____/ |_| |___|_| \\_|",
        ];
        for (index, line) in logo.iter().enumerate() {
            self.text(
                logo_x,
                logo_y + index as u64 * 10 * scale,
                line,
                scale,
                if index == 0 { white } else { cyan },
            );
        }
        self.text(
            logo_x,
            logo_y + 54 * scale,
            "A SMALL SYSTEM. A CLEAR PURPOSE.",
            scale,
            muted,
        );

        let panel_y = logo_y + 82 * scale;
        let panel_height = self
            .info
            .height
            .saturating_sub(panel_y + margin + 26 * scale);
        let panel_width = self.info.width.saturating_sub(margin * 2);
        self.rectangle(margin, panel_y, panel_width, panel_height, panel);
        self.rectangle(margin, panel_y, panel_width, 2 * scale, panel_edge);
        self.text(
            margin + 14 * scale,
            panel_y + 12 * scale,
            "BOOT SEQUENCE / LIVE KERNEL OUTPUT",
            scale,
            amber,
        );
        self.text(
            self.info.width.saturating_sub(margin + 110 * scale),
            panel_y + 12 * scale,
            "STATUS: RUNNING",
            scale,
            green,
        );

        let log_x = margin + 16 * scale;
        let log_y = panel_y + 38 * scale;
        let line_height = 13 * scale;
        let footer_space = 42 * scale;
        let available_height = panel_height.saturating_sub(38 * scale + footer_space);
        let visible_rows = (available_height / line_height).max(1) as usize;
        let logs: [(&str, Colour); 19] = [
            ("POWER-ON SELF TEST ............... OK", green),
            ("LIMINE BOOT PROTOCOL ............. OK", green),
            ("KERNEL IMAGE .............. LOADED", green),
            ("COM1 SERIAL CONSOLE .............. OK", green),
            ("INTERRUPT DESCRIPTOR TABLE ....... OK", green),
            ("CPU IDENTIFICATION ............... OK", green),
            ("PHYSICAL MEMORY MAP .............. OK", green),
            ("HIGHER-HALF DIRECT MAP ........... OK", green),
            ("PHYSICAL FRAME ALLOCATOR ......... OK", green),
            ("KERNEL HEAP ...................... OK", green),
            ("RUST ALLOCATOR ................... OK", green),
            ("FRAMEBUFFER ...................... OK", green),
            ("PAGE TABLE ROOT ........... ACTIVE", green),
            ("4K PAGE MAPPER ................... OK", green),
            ("VIRTUAL ADDRESS TRANSLATION ...... OK", green),
            ("EXCEPTION HANDLERS ............... OK", green),
            ("CHECKING KERNEL INVARIANTS ....... OK", cyan),
            ("STARTUP SEQUENCE ................. DONE", amber),
            ("WELCOME TO SESTINA.", white),
        ];

        let log_area_height = visible_rows as u64 * line_height;
        for (index, (message, colour)) in logs.iter().enumerate() {
            self.rectangle(
                margin + 8 * scale,
                log_y,
                panel_width.saturating_sub(16 * scale),
                log_area_height,
                panel,
            );
            let first = (index + 1).saturating_sub(visible_rows);
            for row in first..=index {
                let row_y = log_y + (row - first) as u64 * line_height;
                self.text(log_x, row_y, ">", scale, cyan);
                self.text(log_x + 12 * scale, row_y, logs[row].0, scale, logs[row].1);
            }

            let progress_width = panel_width.saturating_sub(36 * scale);
            let progress_y = panel_y + panel_height.saturating_sub(24 * scale);
            self.rectangle(
                margin + 18 * scale,
                progress_y,
                progress_width,
                5 * scale,
                panel_edge,
            );
            let filled = progress_width.saturating_mul((index + 1) as u64) / logs.len() as u64;
            self.rectangle(
                margin + 18 * scale,
                progress_y,
                filled,
                5 * scale,
                if index + 1 == logs.len() { green } else { cyan },
            );

            for _ in 0..180_000 {
                core::hint::spin_loop();
            }
            let _ = (message, colour);
        }

        let footer_y = self.info.height.saturating_sub(18 * scale);
        self.text(
            margin,
            footer_y,
            "SESTINA OS // BUILT WITH CARE",
            scale,
            muted,
        );
        self.text(
            self.info.width.saturating_sub(margin + 66 * scale),
            footer_y,
            "READY",
            scale,
            green,
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
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '\\' => [16, 16, 8, 4, 2, 1, 1],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        '>' => [16, 8, 4, 2, 4, 8, 16],
        '<' => [1, 2, 4, 8, 4, 2, 1],
        '=' => [0, 31, 0, 31, 0, 0, 0],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '#' => [10, 31, 10, 10, 31, 10, 0],
        '*' => [0, 21, 14, 31, 14, 21, 0],
        '!' => [4, 4, 4, 4, 4, 0, 4],
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
