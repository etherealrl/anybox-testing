use super::*;
use crate::person::View;

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
                    if ui.button("start").clicked() {
                        self.send(view, Answer::Start);
                    }
                    if ui.button("skip").clicked() {
                        self.send(view, Answer::Skip);
                    }
                });
            }
            Phase::Confirm => {
                if ui.button("ready").clicked() {
                    self.send(view, Answer::Start);
                }
                if ui.button("skip").clicked() {
                    self.send(view, Answer::Skip);
                }
            }
            Phase::Question => {
                ui.horizontal(|ui| {
                    for (label, answer) in [
                        ("yes", Answer::Yes),
                        ("no", Answer::No),
                        ("not sure", Answer::Skip),
                    ] {
                        if ui.button(label).clicked() {
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
                    if ui
                        .button(if view.reconnect_failed {
                            "skip remaining tests"
                        } else {
                            "next"
                        })
                        .clicked()
                    {
                        self.send(view, Answer::Next);
                    }
                    if ui
                        .button(if view.reconnect_failed {
                            "retry reconnect"
                        } else {
                            "retry"
                        })
                        .clicked()
                    {
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
