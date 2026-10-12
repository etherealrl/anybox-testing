use crate::setup::Setup;
use anybox::Family;
use eframe::egui;

pub fn draw(ui: &mut egui::Ui, setup: &mut Setup) -> bool {
    ui.label("which box?");
    ui.horizontal(|ui| {
        for (family, label) in [
            (Family::Ferrum, "ferrum"),
            (Family::Ysc, "ysc 8k"),
            (Family::MakcuV4, "makcu"),
            (Family::Medius, "medius"),
        ] {
            if ui
                .selectable_label(setup.picked && setup.family == family, label)
                .clicked()
            {
                setup.family = family;
                setup.picked = true;
            }
        }
    });
    ui.label("what's plugged into it?");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut setup.keyboard, false, "mouse");
        ui.selectable_value(&mut setup.keyboard, true, "keyboard");
    });
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
        ui.label("which connector has the cable to this pc?");
        ui.text_edit_singleline(&mut setup.connector);
        ui.label("curves or interpolation on?");
        ui.horizontal(|ui| {
            for choice in ["yes", "no", "not sure"] {
                ui.selectable_value(&mut setup.curves, choice.into(), choice);
            }
        });
        ui.checkbox(
            &mut setup.ysc_keys,
            "check keys without a keyboard, with an optional swap",
        );
        ui.checkbox(&mut setup.ysc_debug, "check debug mode with the ysc tool");
    }
    if setup.family == Family::Ferrum {
        ui.checkbox(&mut setup.ferrum_legacy, "also check the box's own port");
        if setup.ferrum_legacy {
            ui.add(
                egui::TextEdit::singleline(&mut setup.legacy_port)
                    .hint_text("legacy port, like COM7"),
            );
        }
    }
    ui.add_space(12.0);
    ui.label("close games and keep this window in front, because the tests send clicks and keys");
    ui.add_enabled(setup.picked, egui::Button::new("connect"))
        .clicked()
}
