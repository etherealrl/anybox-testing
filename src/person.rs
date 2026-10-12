use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::pad::Seen;

/// closing the window or Ctrl+C, so every wait can stop and the devices get closed on the way out instead of left
/// holding something
pub static STOP: AtomicBool = AtomicBool::new(false);

/// the run was stopped
#[derive(Debug)]
pub struct Stopped;

pub type Run<T> = Result<T, Stopped>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// anybox is doing something, nothing to press
    #[default]
    Busy,
    Ready,
    YesNo,
    Text,
}

pub enum Answer {
    Start,
    Skip,
    Yes,
    No,
    Text(String),
}

/// what the window shows right now, the checks write it and the window draws it
#[derive(Default)]
pub struct View {
    pub section: String,
    pub text: String,
    pub hint: String,
    pub mode: Mode,
    /// a timed part, from when to when
    pub now: Option<(Instant, Instant)>,
    pub status: Vec<String>,
    /// the report's path once it's written
    pub done: Option<String>,
}

/// the window's side: the view, the answers coming back, and every input Windows delivered to it
pub struct Ui {
    pub view: Mutex<View>,
    pub answer: Sender<Answer>,
    answers: Mutex<Receiver<Answer>>,
    pub pad: Mutex<Vec<Seen>>,
    pub ctx: OnceLock<eframe::egui::Context>,
}

impl Ui {
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

/// the person at the PC, through the window. in a dry run there's no window, a script answers for them and every
/// wait is a lot shorter
pub struct Person {
    pub ui: Option<Arc<Ui>>,
}

impl Person {
    pub fn scripted(&self) -> bool {
        self.ui.is_none()
    }

    /// a line under the current step, for what's happening right now
    pub fn say(&self, text: &str) {
        match &self.ui {
            Some(ui) => ui.change(|view| {
                view.status.push(text.to_string());
            }),
            None => println!("{text}"),
        }
    }

    /// the start of a box's part
    pub fn heading(&self, title: &str) {
        match &self.ui {
            Some(ui) => ui.change(|view| {
                view.section = title.to_string();
                view.status.clear();
            }),
            None => println!("\n=== {title} ==="),
        }
    }

    fn ask(&self, ui: &Ui, mode: Mode, text: &str, hint: &str) -> Run<Answer> {
        ui.change(|view| {
            view.text = text.trim().to_string();
            view.hint = hint.to_string();
            view.mode = mode;
            view.now = None;
            view.status.clear();
        });
        let answers = ui.answers.lock().unwrap();
        // an answer left over from a double click on the step before isn't this step's
        while answers.try_recv().is_ok() {}
        loop {
            self.stopped()?;
            match answers.recv_timeout(Duration::from_millis(100)) {
                Ok(answer) => {
                    ui.change(|view| view.mode = Mode::Busy);
                    return Ok(answer);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(Stopped),
            }
        }
    }

    /// shows a step and waits for Start. false when they skipped it
    pub fn ready(&self, text: &str) -> Run<bool> {
        match &self.ui {
            Some(ui) => Ok(matches!(
                self.ask(ui, Mode::Ready, text, "")?,
                Answer::Start
            )),
            None => {
                println!("\n{text}");
                Ok(true)
            }
        }
    }

    /// none when they skipped it
    pub fn yes_no(&self, question: &str) -> Run<Option<bool>> {
        match &self.ui {
            Some(ui) => Ok(match self.ask(ui, Mode::YesNo, question, "")? {
                Answer::Yes => Some(true),
                Answer::No => Some(false),
                _ => None,
            }),
            None => {
                println!("\n{question} y");
                Ok(Some(true))
            }
        }
    }

    pub fn answer(&self, question: &str, hint: &str, scripted: &str) -> Run<String> {
        match &self.ui {
            Some(ui) => Ok(match self.ask(ui, Mode::Text, question, hint)? {
                Answer::Text(text) => text.trim().to_string(),
                _ => String::new(),
            }),
            None => {
                println!("\n{question} {scripted}");
                Ok(scripted.to_string())
            }
        }
    }

    /// a timed part starts, the window shows NOW with a countdown and it beeps once high
    pub fn go(&self, wait: Duration) {
        match &self.ui {
            Some(ui) => {
                let start = Instant::now();
                ui.change(|view| view.now = Some((start, start + wait)));
                beep(1500, 200);
            }
            None => println!(">>> NOW"),
        }
    }

    /// and ends, two low beeps
    pub fn stop(&self) {
        match &self.ui {
            Some(ui) => {
                ui.change(|view| view.now = None);
                beep(500, 150);
                std::thread::sleep(Duration::from_millis(80));
                beep(500, 150);
            }
            None => println!(">>> done"),
        }
    }

    pub fn stopped(&self) -> Run<()> {
        if STOP.load(Ordering::Acquire) {
            Err(Stopped)
        } else {
            Ok(())
        }
    }

    /// a dry run waits a twentieth of the time, the mock box needs no time to react
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

    /// forgets what the window saw so far, so the next `seen` only has what came after
    pub fn pad_clear(&self) {
        if let Some(ui) = &self.ui {
            ui.pad.lock().unwrap().clear();
        }
    }

    /// what the window saw since the last clear. none in a dry run, there's no window to see anything
    pub fn seen(&self) -> Option<Vec<Seen>> {
        self.ui
            .as_ref()
            .map(|ui| std::mem::take(&mut *ui.pad.lock().unwrap()))
    }
}

// the terminal's bell is silent in a lot of terminals, Beep goes through the sound card
#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn Beep(frequency: u32, millis: u32) -> i32;
}

fn beep(frequency: u32, millis: u32) {
    #[cfg(windows)]
    // SAFETY: Beep takes two plain numbers
    unsafe {
        Beep(frequency, millis);
    }
    #[cfg(not(windows))]
    let _ = (frequency, millis);
}
