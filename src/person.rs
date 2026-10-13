use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::pad::Seen;
use crate::setup::Setup;

pub static STOP: AtomicBool = AtomicBool::new(false);
#[derive(Debug)]
pub struct Stopped;
pub type Run<T> = Result<T, Stopped>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Setup,
    Connecting,
    Card,
    Positioning,
    Countdown,
    Active,
    Release,
    Question,
    Text,
    Result,
    CleaningUp,
    Finished,
    Save,
    Confirm,
}

#[derive(Debug)]
pub enum Answer {
    Start,
    Skip,
    Yes,
    No,
    Text(String),
    Setup(Setup),
    Next,
    Retry,
}

pub struct Reply {
    pub step: u64,
    pub answer: Answer,
}

#[derive(Clone, Default)]
pub struct Card {
    pub title: String,
    pub result: String,
}

#[derive(Clone, Default)]
pub struct View {
    /// what the window thinks is still held, so a stuck button can be named and cleared
    pub held: String,
    pub section: String,
    pub text: String,
    pub hint: String,
    pub phase: Phase,
    pub step: u64,
    pub timer: Option<(Instant, Instant)>,
    pub status: Vec<String>,
    pub cards: Vec<Card>,
    pub done: Option<Result<String, String>>,
    pub keyboard: bool,
    pub missing: Vec<anybox::Button>,
    pub physical_buttons: Vec<anybox::Button>,
    pub side_choice: bool,
    pub focused: bool,
    pub in_area: bool,
    pub released: bool,
    pub require_area: bool,
    pub cleanup_failed: bool,
    pub restore_debug: bool,
    pub can_position: bool,
    pub reconnect_failed: bool,
}

pub struct Ui {
    pub view: Mutex<View>,
    pub answer: Sender<Reply>,
    answers: Mutex<Receiver<Reply>>,
    pub pad: Mutex<Vec<Seen>>,
    pub ctx: OnceLock<eframe::egui::Context>,
}

impl Ui {
    pub fn poll(&self) -> Option<Answer> {
        let step = self.view.lock().unwrap().step;
        let answers = self.answers.lock().unwrap();
        while let Ok(reply) = answers.try_recv() {
            if reply.step == step {
                return Some(reply.answer);
            }
        }
        None
    }
    pub fn new() -> Arc<Self> {
        let (answer, answers) = channel();
        Arc::new(Self {
            view: Mutex::new(View::default()),
            answer,
            answers: Mutex::new(answers),
            pad: Mutex::new(Vec::new()),
            ctx: OnceLock::new(),
        })
    }

    pub fn change(&self, change: impl FnOnce(&mut View)) {
        change(&mut self.view.lock().unwrap());
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }
}

pub struct Person {
    pub ui: Option<Arc<Ui>>,
}

impl Person {
    pub fn scripted(&self) -> bool {
        self.ui.is_none()
    }
    pub fn keyboard(&self) -> bool {
        self.ui
            .as_ref()
            .is_some_and(|ui| ui.view.lock().unwrap().keyboard)
    }
    pub fn can_position(&self) -> bool {
        self.ui
            .as_ref()
            .is_none_or(|ui| ui.view.lock().unwrap().can_position)
    }
    pub fn confirm(&self, text: &str) -> Run<bool> {
        if self.scripted() {
            return Ok(true);
        }
        Ok(matches!(self.ask(Phase::Confirm, text, "")?, Answer::Start))
    }
    pub fn missing(&self, button: anybox::Button) -> bool {
        self.ui
            .as_ref()
            .is_some_and(|ui| ui.view.lock().unwrap().missing.contains(&button))
    }
    pub fn say(&self, text: &str) {
        if let Some(ui) = &self.ui {
            ui.change(|v| {
                v.status.push(text.to_string());
            });
        } else {
            println!("{text}");
        }
    }
    pub fn heading(&self, title: &str) {
        if let Some(ui) = &self.ui {
            ui.change(|v| {
                v.section = title.to_string();
                v.status.clear();
            });
        } else {
            println!("\n=== {title} ===");
        }
    }
    pub fn cue(&self, text: &str) {
        if let Some(ui) = &self.ui {
            ui.change(|v| v.text = text.to_string());
        } else {
            println!(">>> {text}");
        }
    }
    pub fn ask(&self, phase: Phase, text: &str, hint: &str) -> Run<Answer> {
        let Some(ui) = &self.ui else {
            return Ok(Answer::Start);
        };
        let mut step = 0;
        ui.change(|v| {
            v.step += 1;
            step = v.step;
            v.text = text.trim().to_string();
            v.hint = hint.to_string();
            v.phase = phase;
            v.timer = None;
        });
        let answers = ui.answers.lock().unwrap();
        loop {
            if phase != Phase::Save {
                self.stopped()?;
            }
            match answers.recv_timeout(Duration::from_millis(50)) {
                Ok(reply) if reply.step == step => {
                    ui.change(|v| v.phase = Phase::Active);
                    return Ok(reply.answer);
                }
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(Stopped),
            }
        }
    }
    pub fn ready(&self, text: &str) -> Run<bool> {
        if self.scripted() {
            println!("\n{text}");
            return Ok(true);
        }
        if !matches!(self.ask(Phase::Card, text, "")?, Answer::Start) {
            return Ok(false);
        }
        self.countdown()?;
        Ok(true)
    }
    pub fn countdown(&self) -> Run<()> {
        let Some(ui) = &self.ui else {
            return Ok(());
        };
        ui.change(|v| {
            v.phase = Phase::Positioning;
            v.text = if v.require_area {
                "put your cursor in the test area and let go of everything".into()
            } else {
                "keep this window in front and let go of everything".into()
            };
        });
        loop {
            self.stopped()?;
            let v = ui.view.lock().unwrap().clone();
            if v.focused && v.released && (!v.require_area || v.in_area) {
                break;
            }
            let wanted = if !v.focused {
                "click on this window so it's in front".to_string()
            } else if !v.released {
                format!(
                    "let go of everything. windows thinks {} is still held, press it once to clear it",
                    v.held
                )
            } else {
                "put your cursor in the test area".to_string()
            };
            if v.text != wanted {
                ui.change(|v| v.text = wanted);
            }
            self.wait(Duration::from_millis(50))?;
        }
        for second in [2, 1] {
            ui.change(|v| {
                v.phase = Phase::Countdown;
                v.text = format!("starts in {second}, hands off until it tells you what to do");
            });
            self.wait(Duration::from_secs(1))?;
        }
        self.pad_clear();
        self.action("don't touch anything", Duration::from_millis(500));
        self.wait(Duration::from_millis(500))?;
        if !ui.view.lock().unwrap().released {
            return Err(Stopped);
        }
        self.pad_clear();
        Ok(())
    }
    pub fn yes_no(&self, question: &str) -> Run<Option<bool>> {
        if self.scripted() {
            return Ok(Some(true));
        }
        Ok(match self.ask(Phase::Question, question, "")? {
            Answer::Yes => Some(true),
            Answer::No => Some(false),
            _ => None,
        })
    }
    pub fn answer(&self, question: &str, hint: &str, scripted: &str) -> Run<String> {
        if self.scripted() {
            return Ok(scripted.to_string());
        }
        Ok(match self.ask(Phase::Text, question, hint)? {
            Answer::Text(s) => s.trim().to_string(),
            _ => String::new(),
        })
    }
    pub fn action(&self, text: &str, wait: Duration) {
        if let Some(ui) = &self.ui {
            let start = Instant::now();
            ui.change(|v| {
                v.phase = Phase::Active;
                v.text = text.to_string();
                v.timer = Some((start, start + wait));
            });
            std::thread::spawn(|| beep(1500, 200));
        } else {
            println!(">>> {text}");
        }
    }
    pub fn stop(&self) {
        if let Some(ui) = &self.ui {
            ui.change(|v| {
                v.phase = Phase::Release;
                v.text = "done, you can stop".into();
            });
            std::thread::spawn(|| {
                beep(500, 150);
                std::thread::sleep(Duration::from_millis(80));
                beep(500, 150);
            });
        }
    }
    pub fn result(&self, text: &str) -> Run<bool> {
        if self.scripted() {
            return Ok(false);
        }
        Ok(matches!(self.ask(Phase::Result, text, "")?, Answer::Retry))
    }
    pub fn stopped(&self) -> Run<()> {
        if STOP.load(Ordering::Acquire) {
            return Err(Stopped);
        }
        if let Some(ui) = &self.ui {
            let v = ui.view.lock().unwrap();
            if v.phase == Phase::Active
                && (!v.focused || (v.require_area && !v.in_area && !v.side_choice))
            {
                return Err(Stopped);
            }
        }
        Ok(())
    }
    pub fn scaled(&self, wait: Duration) -> Duration {
        if self.scripted() { wait / 20 } else { wait }
    }
    pub fn wait(&self, wait: Duration) -> Run<()> {
        let until = Instant::now() + self.scaled(wait);
        loop {
            self.stopped()?;
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            std::thread::sleep(left.min(Duration::from_millis(20)));
        }
    }
    pub fn pad_clear(&self) {
        if let Some(ui) = &self.ui {
            ui.pad.lock().unwrap().clear();
        }
    }
    pub fn seen(&self) -> Option<Vec<Seen>> {
        self.ui.as_ref().map(|ui| ui.pad.lock().unwrap().clone())
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn Beep(frequency: u32, millis: u32) -> i32;
}
fn beep(frequency: u32, millis: u32) {
    #[cfg(windows)]
    // safety: Beep takes two plain numbers
    unsafe {
        Beep(frequency, millis);
    }
    #[cfg(not(windows))]
    let _ = (frequency, millis);
}

#[cfg(test)]
mod tests;
