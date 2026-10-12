mod controls;
mod setup;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::pad::{Input, Seen};
use crate::person::{Answer, Phase, Reply, STOP, Ui};
use anybox::Button;
use eframe::egui::{self, Color32, Event, PointerButton, Pos2, RichText, Sense, Stroke};

pub struct App {
    ui: Arc<Ui>,
    worker: Option<JoinHandle<()>>,
    trail: VecDeque<Pos2>,
    typed: String,
    sink: String,
    shown: u64,
    setup: crate::setup::Setup,
    exiting: bool,
    area: Option<egui::Rect>,
}

impl App {
    pub fn new(ui: Arc<Ui>, worker: JoinHandle<()>) -> Self {
        Self {
            ui,
            worker: Some(worker),
            trail: VecDeque::new(),
            typed: String::new(),
            sink: String::new(),
            shown: 0,
            setup: crate::setup::Setup::default(),
            exiting: false,
            area: None,
        }
    }
    fn record(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let (events, focused, released, position, shift, held) = ctx.input(|i| {
            let mut held: Vec<String> = [
                (PointerButton::Primary, "left"),
                (PointerButton::Secondary, "right"),
                (PointerButton::Middle, "middle"),
                (PointerButton::Extra1, "the back side button"),
                (PointerButton::Extra2, "the front side button"),
            ]
            .into_iter()
            .filter(|(button, _)| i.pointer.button_down(*button))
            .map(|(_, name)| name.to_string())
            .collect();
            held.extend(i.keys_down.iter().map(|key| format!("{key:?}")));
            if i.modifiers.shift {
                held.push("shift".into());
            }
            (
                i.raw.events.clone(),
                i.focused,
                !i.pointer.any_down() && i.keys_down.is_empty() && i.modifiers.is_none(),
                i.pointer.hover_pos(),
                i.modifiers.shift,
                held.join(", "),
            )
        });
        self.ui.change(|v| {
            v.focused = focused;
            v.released = released;
            v.held = held;
            v.in_area = self
                .area
                .is_some_and(|r| position.is_some_and(|p| r.contains(p)));
        });
        let phase = self.ui.view.lock().unwrap().phase;
        if !matches!(phase, Phase::Active | Phase::Release | Phase::CleaningUp) {
            return;
        }
        let mut pad = self.ui.pad.lock().unwrap();
        pad.push(Seen {
            at: now,
            input: Input::Shift(shift),
        });
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
                Event::MouseWheel { unit, delta, .. } => {
                    pad.push(Seen {
                        at: now,
                        input: Input::WheelDetail(format!("{unit:?} {} {}", delta.x, delta.y)),
                    });
                    Input::Wheel(delta.y)
                }
                Event::MouseMoved(delta) => Input::Moved(delta.x, delta.y),
                Event::Key {
                    key,
                    pressed,
                    repeat: false,
                    ..
                } => Input::Key {
                    key: format!("{key:?}"),
                    pressed,
                },
                Event::Text(text) => {
                    if self.sink.len() < 32 {
                        self.sink
                            .extend(text.chars().filter(|c| "abcABC".contains(*c)).take(3));
                    }
                    Input::Text(text)
                }
                Event::Paste(_) => Input::Paste,
                _ => continue,
            };
            if pad.len() < 20000 {
                pad.push(Seen { at: now, input });
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) {
            self.exiting = true;
            let current = self.ui.view.lock().unwrap().clone();
            if current.phase == Phase::Save {
                self.send(&current, Answer::Skip);
            }
            if self.ui.view.lock().unwrap().done.is_none() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                STOP.store(true, Ordering::Release);
                self.ui.change(|v| {
                    v.phase = Phase::CleaningUp;
                    v.text = "letting go and saving the report".into();
                });
            }
        }
        self.record(&ctx);
        ctx.request_repaint_after(Duration::from_millis(30));
        let view = self.ui.view.lock().unwrap().clone();
        if self.exiting && view.done.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.shown != view.step {
            self.shown = view.step;
            self.typed.clear();
            self.sink.clear();
            if !matches!(view.phase, Phase::Setup | Phase::Text | Phase::Save) {
                ctx.memory_mut(|m| {
                    if let Some(id) = m.focused() {
                        m.surrender_focus(id);
                    }
                });
            }
        }
        let sender = self.ui.answer.clone();
        let answer = |a| {
            let _ = sender.send(Reply {
                step: view.step,
                answer: a,
            });
        };
        egui::Panel::top("alert")
            .exact_size(140.0)
            .show(root, |ui| {
                ui.label(RichText::new(&view.section).size(16.0));
                // a card only says what the next test is about, green is kept for what to do right now
                if view.phase == Phase::Card {
                    ui.label(
                        RichText::new(
                            "next test, don't do anything yet, it tells you what to do after start",
                        )
                        .size(16.0)
                        .weak(),
                    );
                    ui.label(RichText::new(&view.text).size(26.0).strong());
                } else {
                    ui.label(
                        RichText::new(&view.text)
                            .size(28.0)
                            .strong()
                            .color(Color32::from_rgb(110, 210, 160)),
                    );
                }
                if let Some((start, end)) = view.timer {
                    let total = (end - start).as_secs_f32().max(0.001);
                    ui.add(
                        egui::ProgressBar::new((start.elapsed().as_secs_f32() / total).min(1.0))
                            .desired_height(18.0),
                    );
                }
            });
        if !matches!(
            view.phase,
            Phase::Setup | Phase::Finished | Phase::Connecting
        ) {
            egui::Panel::left("tests")
                .exact_size(220.0)
                .show(root, |ui| {
                    ui.label(format!(
                        "test {} of {}",
                        view.cards
                            .iter()
                            .filter(|c| c.result != "waiting")
                            .count()
                            .min(view.cards.len()),
                        view.cards.len()
                    ));
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for card in &view.cards {
                            ui.label(&card.title);
                            ui.small(&card.result);
                            ui.add_space(6.0);
                        }
                    });
                });
        }
        egui::CentralPanel::default_margins().show(root, |ui| {
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 230.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.controls(ui, &view);
                ui.label(&view.hint);
                if view.side_choice && view.phase == Phase::Active && ui.button("this mouse doesn't have it").clicked() { answer(Answer::No); }
                if view.done.is_none() && !matches!(view.phase, Phase::Setup | Phase::Finished) && ui.button("stop run").clicked() {
                    STOP.store(true, Ordering::Release);
                    self.ui.change(|v| { v.phase = Phase::CleaningUp; v.text = "letting go and saving the report".into(); });
                }
                if view.cleanup_failed { ui.label("if anything is still held or locked, unplug the box and plug it back in"); }
                if view.restore_debug { ui.label("turn debug mode off, then close the ysc tool before you leave"); }
                if !matches!(view.phase, Phase::Setup | Phase::Active | Phase::Countdown | Phase::Release | Phase::CleaningUp) {
                    ui.collapsing("details", |ui| { egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| { for line in &view.status { ui.label(line); } }); });
                }
            });
            // the test area means nothing before a box is connected or after the run
            if matches!(view.phase, Phase::Setup | Phase::Connecting | Phase::Finished) {
                return;
            }
            ui.label(if view.keyboard { "type here" } else { "test area" });
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            self.area = Some(rect);
            let painter = ui.painter_at(rect);
            painter.rect_stroke(rect, 8.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
            if let Some(pos) = response.hover_pos() { self.trail.push_back(pos); if self.trail.len() > 300 { self.trail.pop_front(); } }
            painter.add(egui::Shape::line(self.trail.iter().copied().collect(), Stroke::new(2.0, Color32::from_rgb(90,160,255))));
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, if view.keyboard { &self.sink } else { "put your cursor here" }, egui::FontId::proportional(20.0), Color32::GRAY);
        });
    }
    fn on_exit(&mut self) {
        STOP.store(true, Ordering::Release);
        let view = self.ui.view.lock().unwrap().clone();
        if view.phase == Phase::Save {
            self.send(&view, Answer::Skip);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests;
