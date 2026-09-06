use eframe::egui;
use framework_lib::chromium_ec::{CrosEc, CrosEcDriver, EcError};
use framework_lib::chromium_ec::commands::RgbS;

const COLOR_COUNT: usize = 8;
const START_KEY: u8 = 0;
const PRESET_SPECTRUM: [u32; COLOR_COUNT] = [
    0xFF0000, 0xFF7F00, 0xFFFF00, 0x00FF00, 0x0000FF, 0x4B0082, 0x9400D3, 0xFFFFFF,
];
const PRESET_MATRIX: [u32; COLOR_COUNT] = [
    0x006622, 0x00FF66, 0x009944, 0x00CC44, 0x00CC66, 0x009933, 0x006633, 0x00FF99,
];
const PRESET_AZURE: [u32; COLOR_COUNT] = [
    0x2B2BFF, 0x0C0CFF, 0x4370FF, 0x1A1AFF, 0x5880FF, 0x3C3CFF, 0x2E5FFF, 0x1A4FFF,
];
const PRESET_NEON_CITY: [u32; COLOR_COUNT] = [
    0xFF0099, 0xFF00FF, 0x8A2BE2, 0x00FFFF, 0xFF1493, 0x00CCFF, 0x9400D3, 0x00BFFF,
];
const PRESET_SOLAR_FLARE: [u32; COLOR_COUNT] = [
    0xFFD700, 0xFF4500, 0xFF7F50, 0xFFA500, 0xFFFF00, 0xFF8C00, 0xFF6347, 0xFFD700,
];
const PRESET_ABYSS: [u32; COLOR_COUNT] = [
    0x0000FF, 0x191970, 0x4169E1, 0x000080, 0x1E90FF, 0x00008B, 0x00BFFF, 0x0000CD,
];
const PRESET_CANOPY: [u32; COLOR_COUNT] = [
    0x32CD32, 0x006400, 0x8FBC8F, 0x228B22, 0x556B2F, 0x90EE90, 0x6B8E23, 0x008000,
];
const PRESET_CYANO: [u32; COLOR_COUNT] = [
    0x006680, 0x001A33, 0x4DD556, 0x00334D, 0x1A9C6E, 0x00807A, 0x33B862, 0x004D66,
];
const PRESET_ALGA: [u32; COLOR_COUNT] = [
    0x006680, 0x0C0CFF, 0x4DD556, 0x1A1AFF, 0x1A9C6E, 0x3C3CFF, 0x33B862, 0x1A4FFF,
];

/// Convert a raw 24-bit RGB value into the EC payload struct.
fn rgb_from_u32(value: u32) -> RgbS {
    RgbS {
        r: ((value & 0x00FF_0000) >> 16) as u8,
        g: ((value & 0x0000_FF00) >> 8) as u8,
        b: (value & 0x0000_00FF) as u8,
    }
}

/// Apply RGB colors starting at a given key index using the Framework EC.
fn apply_colors(colors: Vec<RgbS>) -> Result<(), EcError> {
    let ec = CrosEc::new();
    ec.rgbkbd_set_color(START_KEY, colors)
}

/// Set the main fan's duty cycle (0-100%) using the Framework EC's `PwmSetFanDuty` command.
/// Note this differs from the generic PWM_SET_DUTY command — fan duty takes only the
/// percentage, with no leading index byte. This command returns no response body.
fn set_fan_duty(duty_percent: u8) -> Result<(), EcError> {
    let ec = CrosEc::new();

    // Clamp into the valid 0-100% range before sending to the EC.
    let duty = duty_percent.min(100);

    // `percent` is a 32-bit little-endian value, not a single byte — send all four
    // bytes so the EC reads the payload correctly (same as framework-tool / ectool).
    let request = (duty as u32).to_le_bytes();

    // `send_command` comes from the `CrosEcDriver` trait (now in scope). This command
    // has no response body, so we use the low-level path and ignore any returned bytes.
    ec.send_command(0x0024u16, 0, &request)?;
    Ok(())
}

/// Convert a `RgbS` color to a hex string (`#RRGGBB`).
fn rgb_to_hex_string(color: RgbS) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

/// Provide a user-friendly explanation for an EC error, including privilege guidance.
fn format_ec_error(err: &EcError) -> String {
    match err {
        EcError::DeviceError(message) if message.contains("Not a Framework Laptop") => {
            "EC access denied: SMBIOS check failed. Run this tool with administrative \
privileges (sudo) on a Framework system so the EC can be reached."
                .to_string()
        }
        EcError::DeviceError(message) => format!("EC device error: {message}"),
        EcError::Response(status) => {
            format!("EC responded with status {:?}", status)
        }
        EcError::UnknownResponseCode(code) => {
            format!("EC returned unknown response code 0x{code:X}.")
        }
    }
}

fn color32_from_rgb(color: RgbS) -> egui::Color32 {
    egui::Color32::from_rgb(color.r, color.g, color.b)
}

fn rgb_from_color32(color: egui::Color32) -> RgbS {
    RgbS {
        r: color.r(),
        g: color.g(),
        b: color.b(),
    }
}

#[derive(Clone, Copy)]
enum StatusKind {
    Info,
    Success,
    Error,
}

struct StatusMessage {
    kind: StatusKind,
    text: String,
}

struct FanRgbApp {
    fan_duty: u8,
    colors: [egui::Color32; COLOR_COUNT],
    status: Option<StatusMessage>,
    lights_enabled: bool,
    /// When `true`, "Write to controller" updates only the LEDs and leaves the
    /// fan controller untouched. When `false`, it writes both LEDs and the fan.
    led_only: bool,
}

impl FanRgbApp {
    fn new() -> Self {
        let colors = PRESET_SPECTRUM
            .iter()
            .map(|color| color32_from_rgb(rgb_from_u32(*color)))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or([egui::Color32::BLACK; COLOR_COUNT]);

        Self {
            // 0 means "no fan choice made yet" — the fan is only written once the
            // user explicitly picks 50% or 100%, so we never surprise them.
            fan_duty: 0,
            colors,
            status: Some(StatusMessage {
                kind: StatusKind::Info,
                text: "Pick colors and settings, then press 'Write to controller'.".to_string(),
            }),
            lights_enabled: true,
            led_only: true,
        }
    }

    fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        self.status = Some(StatusMessage {
            kind,
            text: text.into(),
        });
    }

    fn current_colors(&self) -> Vec<RgbS> {
        self.colors.iter().copied().map(rgb_from_color32).collect()
    }

    fn apply_palette(&mut self, palette: &[u32]) {
        for idx in 0..COLOR_COUNT {
            let value = palette[idx % palette.len()];
            self.colors[idx] = color32_from_rgb(rgb_from_u32(value));
        }
        self.lights_enabled = true;
    }

    /// Write the current colors (and, unless `led_only` is set, the fan duty) to the EC.
    fn apply(&mut self) {
        // Lighting step: either turn LEDs off or write the chosen colors.
        if !self.lights_enabled {
            match self.turn_off_lights() {
                Ok(message) => self.set_status(StatusKind::Info, message),
                Err(err) => self.set_status(StatusKind::Error, err),
            }
            return;
        }

        // Fan step: only override the fan if one was actually chosen AND the user
        // hasn't asked for LED-only writes.
        if !self.led_only {
            match set_fan_duty(self.fan_duty) {
                Ok(()) => {}
                Err(err) => {
                    self.set_status(StatusKind::Error, format_ec_error(&err));
                    return;
                }
            }
        }

        // Colors step: report success with a combined message.
        match apply_colors(self.current_colors()) {
            Ok(()) => self.set_status(
                StatusKind::Success,
                if self.led_only {
                    format!("Wrote {} colors", COLOR_COUNT)
                } else {
                    format!(
                        "Wrote {} colors, main fan at {}%",
                        COLOR_COUNT, self.fan_duty
                    )
                },
            ),
            Err(err) => self.set_status(StatusKind::Error, format_ec_error(&err)),
        }
    }

    fn turn_off_lights(&mut self) -> Result<String, String> {
        let off = vec![RgbS { r: 0, g: 0, b: 0 }; COLOR_COUNT];
        apply_colors(off)
            .map(|_| "Fan lighting disabled".to_string())
            .map_err(|err| format_ec_error(&err))
    }
}

impl eframe::App for FanRgbApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add(
                egui::Label::new(
                    "Note: This program must be ran as root and is linux only!"
                )
                .wrap(),
            );
        });

        egui::SidePanel::right("presets_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Presets");

                if ui.button("Spectrum").clicked() {
                    self.apply_palette(&PRESET_SPECTRUM);
                }
                if ui.button("Matrix").clicked() {
                    self.apply_palette(&PRESET_MATRIX);
                }
                if ui.button("Azure").clicked() {
                    self.apply_palette(&PRESET_AZURE);
                }
                if ui.button("Neon City").clicked() {
                    self.apply_palette(&PRESET_NEON_CITY);
                }
                if ui.button("Solar Flare").clicked() {
                    self.apply_palette(&PRESET_SOLAR_FLARE);
                }
                if ui.button("Abyss").clicked() {
                    self.apply_palette(&PRESET_ABYSS);
                }
                if ui.button("Canopy").clicked() {
                    self.apply_palette(&PRESET_CANOPY);
                }
                if ui.button("Cyanobacteria").clicked() {
                    self.apply_palette(&PRESET_CYANO);
                }
                if ui.button("Alga").clicked() {
                    self.apply_palette(&PRESET_ALGA);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.separator();
            ui.heading("Colors");

            for idx in 0..COLOR_COUNT {
                let mut color_value = self.colors[idx];
                let mut updated = false;

                ui.horizontal(|ui| {
                    ui.label(format!("Zone {}", idx + 1));
                    let mut egui_color = color_value;
                    let response = egui::color_picker::color_edit_button_srgba(
                        ui,
                        &mut egui_color,
                        egui::color_picker::Alpha::Opaque,
                    );

                    if response.changed() {
                        color_value = egui_color;
                        updated = true;
                    }

                    ui.label(rgb_to_hex_string(rgb_from_color32(color_value)));
                });

                if updated {
                    self.colors[idx] = color_value;
                    self.lights_enabled = true;
                } else {
                    self.colors[idx] = color_value;
                }
            }

            ui.separator();
            ui.heading("Fan override");
            ui.horizontal(|ui| {
                // Toggle buttons highlight whichever option is currently selected.
                // `fan_duty == 0` means nothing has been chosen yet, so both stay unhighlighted.
                let btn_50 = egui::Button::new("50%").selected(self.fan_duty == 50);
                if ui.add(btn_50).clicked() {
                    self.fan_duty = 50;
                }

                let btn_100 = egui::Button::new("100%").selected(self.fan_duty == 100);
                if ui.add(btn_100).clicked() {
                    self.fan_duty = 100;
                }
            });

            // Choose whether a write updates the LEDs only, or both LEDs and the fan.
            ui.horizontal(|ui| {
                ui.label("Write to:");
                let btn_leds_only = egui::Button::new("LEDs only").selected(self.led_only);
                if ui.add(btn_leds_only).clicked() {
                    self.led_only = true;
                }

                let btn_both = egui::Button::new("Both LEDs & fan").selected(!self.led_only);
                if ui.add(btn_both).clicked() {
                    self.led_only = false;
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Write to controller").clicked() {
                    self.apply();
                }

                let toggle_label = if self.lights_enabled {
                    "Turn off lighting"
                } else {
                    "Turn on lighting"
                };
                if ui.button(toggle_label).clicked() {
                    self.lights_enabled = !self.lights_enabled;
                }
            });

            if let Some(status) = &self.status {
                ui.separator();
                let color = match status.kind {
                    StatusKind::Info => egui::Color32::LIGHT_GRAY,
                    StatusKind::Success => egui::Color32::from_rgb(0, 200, 83),
                    StatusKind::Error => egui::Color32::from_rgb(209, 71, 78),
                };
                ui.colored_label(color, &status.text);
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([660.0, 480.0])
            .with_min_inner_size([520.0, 360.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Cortinarius: The Framework Desktop Fan Controller!",
        native_options,
        Box::new(|_cc| Ok(Box::new(FanRgbApp::new()))),
    )
}
