use super::*;
use crate::person::View;

const GO: Color32 = Color32::from_rgb(40, 140, 70);
const SKIP: Color32 = Color32::from_rgb(85, 85, 85);
const NO: Color32 = Color32::from_rgb(160, 60, 60);
const AGAIN: Color32 = Color32::from_rgb(170, 120, 30);

/// the buttons that move the run along, big and colored so the right one is obvious at a glance
pub(super) fn big(ui: &mut egui::Ui, text: &str, color: Color32) -> bool {
    ui.add(
        egui::Button::new(RichText::new(text).size(22.0).color(Color32::WHITE))
            .fill(color)
            .min_size(egui::vec2(160.0, 48.0)),
    )
    .clicked()
}

impl App {
    pub(super) fn send(&self, view: &View, answer: Answer) {
        let _ = self.ui.answer.send(Reply {
            step: view.step,
            answer,
        });
    }

    pub(super) fn controls(&mut self, ui: &mut egui::Ui, view: &View) {
        match view.phase {
            Phase::Setup => {
                if setup::draw(ui, &mut self.setup) {
                    self.send(view, Answer::Setup(self.setup.clone()));
                }
                if ui.button("exit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Phase::Card => {
                ui.horizontal(|ui| {
                    if big(ui, "start", GO) {
                        self.send(view, Answer::Start);
                    }
                    if big(ui, "skip", SKIP) {
                        self.send(view, Answer::Skip);
                    }
                });
            }
            Phase::Confirm => {
                ui.horizontal(|ui| {
                    if big(ui, "ready", GO) {
                        self.send(view, Answer::Start);
                    }
                    if big(ui, "skip", SKIP) {
                        self.send(view, Answer::Skip);
                    }
                });
            }
            Phase::Question => {
                ui.horizontal(|ui| {
                    for (label, answer, color) in [
                        ("yes", Answer::Yes, GO),
                        ("no", Answer::No, NO),
                        ("not sure", Answer::Skip, SKIP),
                    ] {
                        if big(ui, label, color) {
                            self.send(view, answer);
                        }
                    }
                });
            }
            Phase::Text => {
                egui::ComboBox::from_id_salt("ports")
                    .selected_text("ports")
                    .show_ui(ui, |ui| {
                        for port in &view.status {
                            ui.selectable_value(&mut self.typed, port.clone(), port);
                        }
                    });
                ui.text_edit_singleline(&mut self.typed);
                if ui.button("ready").clicked() {
                    self.send(view, Answer::Text(self.typed.clone()));
                }
                if ui.button("scan again").clicked() {
                    self.send(view, Answer::Text(String::new()));
                }
                if ui.button("save report").clicked() {
                    self.send(view, Answer::Skip);
                }
            }
            Phase::Result => {
                ui.horizontal(|ui| {
                    let (next, retry) = if view.reconnect_failed {
                        ("skip remaining tests", "retry reconnect")
                    } else {
                        ("next", "retry")
                    };
                    if big(ui, next, GO) {
                        self.send(view, Answer::Next);
                    }
                    if big(ui, retry, AGAIN) {
                        self.send(view, Answer::Retry);
                    }
                });
                if view.reconnect_failed && ui.button("save report").clicked() {
                    STOP.store(true, Ordering::Release);
                }
            }
            Phase::Save => {
                ui.label("folder for the report");
                ui.text_edit_singleline(&mut self.typed);
                if ui.button("save somewhere else").clicked() {
                    self.send(view, Answer::Text(self.typed.clone()));
                }
                if ui.button("close").clicked() {
                    self.send(view, Answer::Skip);
                }
            }
            Phase::Finished => {
                match &view.done {
                    Some(Ok(path)) => {
                        ui.label(format!("report saved, {path}"));
                        ui.label(
                            "read the trace before you share it, it can contain unrelated typing",
                        );
                        if ui.button("open folder").clicked() {
                            let _ = std::process::Command::new("explorer")
                                .arg(format!("/select,{path}"))
                                .spawn();
                        }
                    }
                    Some(Err(error)) => {
                        ui.label(error);
                    }
                    None => {}
                }
                if ui.button("close").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            _ => {}
        }
    }
}
