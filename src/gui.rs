use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anybox::Button;
use eframe::egui::{
    self, Align, Color32, Event, Layout, MouseWheelUnit, PointerButton, Pos2, RichText, Sense,
    Stroke, Vec2,
};

use crate::pad::{Input, Seen};
use crate::person::{Answer, Mode, STOP, Ui};

pub struct App {
    ui: Arc<Ui>,
    worker: Option<JoinHandle<()>>,
    trail: VecDeque<Pos2>,
    typed: String,
    recent: VecDeque<String>,
    /// the step on screen, to tell when a new one starts
    shown: String,
}

impl App {
    pub fn new(ui: Arc<Ui>, worker: JoinHandle<()>) -> Self {
        Self {
            ui,
            worker: Some(worker),
            trail: VecDeque::new(),
            typed: String::new(),
            recent: VecDeque::new(),
            shown: String::new(),
        }
    }

    /// everything Windows delivered this frame goes to the checks, whatever the window is showing
    fn record(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let events = ctx.input(|i| i.raw.events.clone());
        let mut pad = self.ui.pad.lock().unwrap();
        for event in events {
            let input = match event {
                Event::PointerButton {
                    button, pressed, ..
                } => Input::Button {
                    button: match button {
                        PointerButton::Primary => Button::Left,
                        PointerButton::Secondary => Button::Right,
                        PointerButton::Middle => Button::Middle,
                        PointerButton::Extra1 => Button::Side1,
                        PointerButton::Extra2 => Button::Side2,
                    },
                    pressed,
                },
                Event::MouseWheel { unit, delta, .. } => Input::Wheel(match unit {
                    MouseWheelUnit::Point => delta.y / 50.0,
                    _ => delta.y,
                }),
                Event::MouseMoved(delta) => Input::Moved(delta.x, delta.y),
                Event::Key { key, pressed, .. } => Input::Key {
                    key: format!("{key:?}"),
                    pressed,
                },
                Event::Text(text) => Input::Text(text),
                _ => continue,
            };
            if !matches!(input, Input::Moved(..)) {
                self.recent.push_front(format!("{input:?}"));
                self.recent.truncate(8);
            }
            pad.push(Seen { at: now, input });
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        self.record(&ctx);
        // a countdown and the trail need frames even when nothing happens
        ctx.request_repaint_after(Duration::from_millis(30));
        let view = self.ui.view.lock().unwrap();
        let fresh = view.text != self.shown;
        if fresh {
            self.shown = view.text.clone();
            self.trail.clear();
        }
        let answer = |a: Answer| {
            let _ = self.ui.answer.send(a);
        };

        egui::Panel::top("section").show(root, |ui| {
            ui.add_space(6.0);
            ui.heading(RichText::new(&view.section).size(22.0).strong());
            ui.add_space(6.0);
        });

        egui::Panel::left("step")
            .exact_size(460.0)
            .show(root, |ui| {
                ui.add_space(10.0);
                if let Some(path) = &view.done {
                    ui.label(RichText::new("all done").size(22.0).strong());
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "the report is {path}. it's plain text, so you can read it before you send it"
                        ))
                        .size(16.0),
                    );
                    ui.add_space(8.0);
                    if ui.button(RichText::new("show it in its folder").size(16.0)).clicked() {
                        let _ = std::process::Command::new("explorer")
                            .arg(format!("/select,{path}"))
                            .spawn();
                    }
                    return;
                }
                ui.label(RichText::new(&view.text).size(17.0));
                if !view.hint.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&view.hint).size(14.0).weak());
                }
                ui.add_space(12.0);
                let big = |text: &str| egui::Button::new(RichText::new(text).size(18.0)).min_size(Vec2::new(120.0, 36.0));
                match view.mode {
                    Mode::Ready => {
                        ui.horizontal(|ui| {
                            if ui.add(big("Start")).clicked() {
                                answer(Answer::Start);
                            }
                            if ui.add(big("Skip")).clicked() {
                                answer(Answer::Skip);
                            }
                        });
                    }
                    Mode::YesNo => {
                        ui.horizontal(|ui| {
                            if ui.add(big("Yes")).clicked() {
                                answer(Answer::Yes);
                            }
                            if ui.add(big("No")).clicked() {
                                answer(Answer::No);
                            }
                            if ui.add(big("Skip")).clicked() {
                                answer(Answer::Skip);
                            }
                        });
                    }
                    Mode::Text => {
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut self.typed)
                                .font(egui::TextStyle::Heading)
                                .desired_width(400.0),
                        );
                        if fresh {
                            field.request_focus();
                        }
                        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.add(big("OK")).clicked() || entered {
                            answer(Answer::Text(std::mem::take(&mut self.typed)));
                        }
                    }
                    Mode::Busy => {
                        ui.label(RichText::new("anybox is working, hang on").size(15.0).weak());
                    }
                }
                if let Some((start, end)) = view.now {
                    ui.add_space(16.0);
                    ui.label(RichText::new("NOW").size(40.0).strong().color(Color32::from_rgb(80, 200, 120)));
                    let total = (end - start).as_secs_f32().max(0.001);
                    let done = start.elapsed().as_secs_f32().min(total);
                    ui.add(
                        egui::ProgressBar::new(done / total)
                            .text(format!("{:.0} s left", (total - done).max(0.0)))
                            .desired_height(24.0),
                    );
                }
                ui.add_space(12.0);
                for line in &view.status {
                    ui.label(RichText::new(line).size(15.0));
                }
            });

        egui::CentralPanel::default_margins().show(root, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    RichText::new("test area, keep your cursor in here")
                        .size(16.0)
                        .weak(),
                );
            });
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
            let painter = ui.painter_at(rect);
            painter.rect_stroke(
                rect,
                8.0,
                Stroke::new(1.0, Color32::GRAY),
                egui::StrokeKind::Inside,
            );
            if let Some(pos) = response.hover_pos() {
                self.trail.push_back(pos);
                if self.trail.len() > 300 {
                    self.trail.pop_front();
                }
            }
            let points: Vec<Pos2> = self.trail.iter().copied().collect();
            painter.add(egui::Shape::line(
                points,
                Stroke::new(2.0, Color32::from_rgb(90, 160, 255)),
            ));
            let mut y = rect.top() + 12.0;
            for line in &self.recent {
                painter.text(
                    Pos2::new(rect.left() + 12.0, y),
                    egui::Align2::LEFT_TOP,
                    line,
                    egui::FontId::monospace(13.0),
                    Color32::GRAY,
                );
                y += 18.0;
            }
        });
    }

    fn on_exit(&mut self) {
        // closing the window stops the run, and the checks close every box before the worker ends
        STOP.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
