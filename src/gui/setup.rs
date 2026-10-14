use crate::setup::Setup;
use anybox::{Family, FerrumApi};
use eframe::egui;

struct Preset {
    /// the presets with the same box and way in share a row
    row: &'static str,
    family: Family,
    api: FerrumApi,
    keyboard: bool,
    /// what to get ready before connecting
    hint: &'static str,
}

const APP: &str = "keep the ferrum app open, with the device set in it";
const PRESETS: [Preset; 9] = [
    Preset {
        row: "ferrum, through the ferrum app",
        family: Family::Ferrum,
        api: FerrumApi::Software,
        keyboard: false,
        hint: APP,
    },
    Preset {
        row: "ferrum, through the ferrum app",
        family: Family::Ferrum,
        api: FerrumApi::Software,
        keyboard: true,
        hint: APP,
    },
    Preset {
        row: "ferrum, on the box's own port",
        family: Family::Ferrum,
        api: FerrumApi::Legacy,
        keyboard: false,
        hint: "close the ferrum app COMPLETELY first, also from the system tray, otherwise it keeps the box's own port to itself",
    },
    Preset {
        row: "ysc 8k",
        family: Family::Ysc,
        api: FerrumApi::Software,
        keyboard: false,
        hint: "",
    },
    Preset {
        row: "ysc 8k",
        family: Family::Ysc,
        api: FerrumApi::Software,
        keyboard: true,
        hint: "",
    },
    Preset {
        row: "makcu",
        family: Family::MakcuV4,
        api: FerrumApi::Software,
        keyboard: false,
        hint: "",
    },
    Preset {
        row: "makcu",
        family: Family::MakcuV4,
        api: FerrumApi::Software,
        keyboard: true,
        hint: "",
    },
    Preset {
        row: "medius",
        family: Family::Medius,
        api: FerrumApi::Software,
        keyboard: false,
        hint: "",
    },
    Preset {
        row: "medius",
        family: Family::Medius,
        api: FerrumApi::Software,
        keyboard: true,
        hint: "",
    },
];

impl Preset {
    fn picked(&self, setup: &Setup) -> bool {
        setup.picked
            && setup.family == self.family
            && setup.keyboard == self.keyboard
            && (self.family != Family::Ferrum || setup.ferrum_api == self.api)
    }
}

pub fn draw(ui: &mut egui::Ui, setup: &mut Setup) -> bool {
    ui.label("what are you testing?");
    egui::Grid::new("presets").show(ui, |ui| {
        for (index, preset) in PRESETS.iter().enumerate() {
            let starts_row = index == 0 || PRESETS[index - 1].row != preset.row;
            if starts_row {
                if index > 0 {
                    ui.end_row();
                }
                ui.label(preset.row);
            }
            let device = if preset.keyboard { "keyboard" } else { "mouse" };
            if ui.selectable_label(preset.picked(setup), device).clicked() {
                setup.family = preset.family;
                setup.ferrum_api = preset.api;
                setup.keyboard = preset.keyboard;
                setup.picked = true;
            }
        }
        ui.end_row();
    });
    if let Some(preset) = PRESETS.iter().find(|p| p.picked(setup))
        && !preset.hint.is_empty()
    {
        ui.colored_label(egui::Color32::from_rgb(40, 140, 70), preset.hint);
    }
    ui.add_space(8.0);
    if setup.keyboard {
        ui.checkbox(
            &mut setup.can_position,
            "i can position the cursor with a pc mouse or touchpad",
        );
    } else {
        setup.can_position = true;
    }
    ui.label("device name");
    ui.add(egui::TextEdit::singleline(&mut setup.name).hint_text("roughly is fine"));
    ui.checkbox(&mut setup.khz, "it's at 1 khz");
    if !setup.khz {
        ui.label("rate in hz, if you know it");
        ui.add(egui::TextEdit::singleline(&mut setup.rate).hint_text("not sure"));
    }
    if setup.family == Family::Ysc {
        ui.label("is interpolation or a movement curve turned on in the ysc app? they change how moves come out");
        ui.horizontal(|ui| {
            for choice in ["yes", "no", "not sure"] {
                ui.selectable_value(&mut setup.curves, choice.into(), choice);
            }
        });
        egui::CollapsingHeader::new("extra checks, all on by default").show(ui, |ui| {
            ui.checkbox(
                &mut setup.ysc_keys,
                "check keys without a keyboard, with an optional swap",
            );
            ui.checkbox(&mut setup.ysc_debug, "check debug mode with the ysc tool");
        });
    }
    ui.add_space(12.0);
    ui.label("close games and keep this window in front, because the tests send clicks and keys");
    ui.add_enabled(
        setup.picked,
        egui::Button::new(
            egui::RichText::new("connect")
                .size(22.0)
                .color(egui::Color32::WHITE),
        )
        .fill(egui::Color32::from_rgb(40, 140, 70))
        .min_size(egui::vec2(160.0, 48.0)),
    )
    .clicked()
}
