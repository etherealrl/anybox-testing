mod lifecycle;
mod locks;
mod output;
mod physical;
mod receivers;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anybox::{
    Action, Axis, Button, Click, Device, Direction, Error, Event, EventKinds, Events, Key, Quirk,
};

use crate::pad::{self, Seen};
use crate::person::{Person, Run};
use crate::report::{Outcome, Report};

pub const F13: Key = Key(0x68);
pub const F14: Key = Key(0x69);
const BUTTONS: [Button; 5] = [
    Button::Left,
    Button::Right,
    Button::Middle,
    Button::Side1,
    Button::Side2,
];

pub struct Checks<'a> {
    pub device: &'a Device,
    pub person: &'a Person,
    pub report: &'a mut Report,
}

fn supported(device: &Device, action: Action) -> bool {
    device.snapshot().capabilities.get(action).is_supported()
}

fn quirks(device: &Device, action: Action) -> Vec<Quirk> {
    device.snapshot().capabilities.get(action).quirks.clone()
}
pub fn drain(events: &Events) -> Vec<Event> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}
pub fn gather(person: &Person, events: &Events, text: &str, wait: Duration) -> Run<Vec<Event>> {
    person.action(
        if text.is_empty() {
            "hands off, don't touch anything"
        } else {
            text
        },
        wait,
    );
    let mut seen = Vec::new();
    collect(person, events, wait, &mut seen)?;
    person.stop();
    Ok(seen)
}
fn collect(person: &Person, events: &Events, wait: Duration, seen: &mut Vec<Event>) -> Run<()> {
    let until = Instant::now() + person.scaled(wait);
    loop {
        person.stopped()?;
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(());
        }
        if let Ok(event) = events.recv_timeout(left.min(Duration::from_millis(50))) {
            seen.push(event);
        }
    }
}

fn presses_of(events: &[Event], wanted: Button) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::Button { button, pressed: true, .. } if *button == wanted))
        .count()
}

fn buttons_in(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::Button { .. }))
        .count()
}
pub fn tally(events: &[Event]) -> String {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let (mut dx, mut dy, mut wheel, mut pan) = (0i64, 0i64, 0i64, 0i64);
    for event in events {
        let name = match event {
            Event::Button {
                button, pressed, ..
            } => format!("{button:?} {}", if *pressed { "down" } else { "up" }),
            Event::Key { key, pressed, .. } => {
                format!(
                    "key {:#04x} {}",
                    key.0,
                    if *pressed { "down" } else { "up" }
                )
            }
            Event::Media { key, pressed, .. } => {
                format!("media {key:?} {}", if *pressed { "down" } else { "up" })
            }
            Event::Motion {
                dx: x,
                dy: y,
                wheel: w,
                pan: p,
                ..
            } => {
                dx += i64::from(*x);
                dy += i64::from(*y);
                wheel += i64::from(*w);
                pan += i64::from(*p);
                "motion".to_string()
            }
            Event::Dropped { count, source, .. } => {
                *counts.entry(format!("dropped by {source:?}")).or_default() += count;
                continue;
            }
            Event::Rejected { action, count, .. } => {
                *counts.entry(format!("rejected {action}")).or_default() += count;
                continue;
            }
            other => format!("{other:?}"),
        };
        *counts.entry(name).or_default() += 1;
    }
    let mut parts: Vec<String> = counts
        .into_iter()
        .map(|(name, count)| format!("{name} x{count}"))
        .collect();
    if parts.iter().any(|p| p.starts_with("motion")) {
        parts.push(format!(
            "motion totals dx {dx} dy {dy} wheel {wheel} pan {pan}"
        ));
    }
    if parts.is_empty() {
        "nothing came in".to_string()
    } else {
        parts.join(", ")
    }
}

fn result(report: &mut Report, name: &str, result: anybox::Result<()>, detail: &str) {
    match result {
        Ok(()) => report.check(name, Outcome::Passed, detail),
        Err(error) => report.check(name, Outcome::Failed, &format!("{error}")),
    }
}
fn asked(report: &mut Report, name: &str, answer: Option<bool>, detail: &str) {
    let outcome = match answer {
        Some(true) => Outcome::Passed,
        Some(false) => Outcome::Failed,
        None => Outcome::Noted,
    };
    report.check(name, outcome, detail);
}

fn passed_if(ok: bool) -> Outcome {
    if ok { Outcome::Passed } else { Outcome::Failed }
}
fn went(seen: &[Seen], dx: i16, dy: i16) -> bool {
    let (x, y) = pad::moved(seen);
    if dx.unsigned_abs() >= dy.unsigned_abs() {
        x != 0.0 && x.signum() == f32::from(dx).signum()
    } else {
        y != 0.0 && y.signum() == f32::from(dy).signum()
    }
}

impl Checks<'_> {
    pub fn card(&mut self, title: &str, check: impl Fn(&mut Self) -> Run<()>) -> Run<()> {
        let mut index = 0;
        if let Some(ui) = &self.person.ui {
            ui.change(|v| {
                index = v
                    .cards
                    .iter()
                    .position(|c| c.title == title)
                    .unwrap_or(v.cards.len());
                if index == v.cards.len() {
                    v.cards.push(crate::person::Card {
                        title: title.into(),
                        result: "waiting".into(),
                    });
                }
            });
        }
        loop {
            let before = self.report.counts();
            let position = self.report.position();
            if let Some(ui) = &self.person.ui {
                ui.change(|v| {
                    v.section = title.into();
                    v.cards[index].result = "running".into();
                    v.status.clear();
                });
            }
            let ran = check(self);
            let interrupted =
                ran.is_err() && !crate::person::STOP.load(std::sync::atomic::Ordering::Acquire);
            if ran.is_err() && !crate::person::STOP.load(std::sync::atomic::Ordering::Acquire) {
                self.release_inputs();
                self.report.check(
                    title,
                    Outcome::Noted,
                    "capture interrupted, bring the pointer back and retry",
                );
            } else {
                ran?;
            }
            let after = self.report.counts();
            if let Some(ui) = &self.person.ui {
                ui.change(|v| {
                    v.status = self
                        .report
                        .since(position)
                        .lines()
                        .map(str::to_string)
                        .collect()
                });
            }
            let label = if after[1] > before[1] {
                "failed, see details"
            } else if interrupted {
                "interrupted, the window lost focus or the cursor left the test area, press retry"
            } else if after[0] > before[0] {
                "passed, see details"
            } else if after[4] > before[4] {
                "recorded, see details"
            } else if after[3] > before[3] {
                "not available with this setup"
            } else {
                "skipped"
            };
            if let Some(ui) = &self.person.ui {
                ui.change(|v| {
                    v.cards[index].result = label.split(',').next().unwrap_or(label).into()
                });
            }
            if !self.person.result(label)? {
                return Ok(());
            }
            self.report.line(&format!(
                "retrying {title}, the earlier attempt stays above"
            ));
        }
    }

    fn release_inputs(&mut self) {
        if supported(self.device, Action::Release) {
            for button in BUTTONS {
                result(
                    self.report,
                    "release after interrupted capture",
                    self.device.release(button),
                    "",
                );
            }
        }
        if supported(self.device, Action::KeyRelease) {
            for key in [F13, F14, Key::LEFT_SHIFT] {
                result(
                    self.report,
                    "key release after interrupted capture",
                    self.device.release_key(key),
                    "",
                );
            }
        }
        if supported(self.device, Action::LockButton) {
            for button in BUTTONS {
                result(
                    self.report,
                    "unlock after interrupted capture",
                    self.device.unlock_button(button),
                    "",
                );
            }
        }
        if supported(self.device, Action::LockAxis) {
            for axis in [Axis::X, Axis::Y] {
                result(
                    self.report,
                    "axis unlock after interrupted capture",
                    self.device.unlock_axis(axis, Direction::Both),
                    "",
                );
            }
        }
    }
    fn window(&mut self, name: &str, call: anybox::Result<()>, ok: impl FnOnce(&[Seen]) -> bool) {
        if let Err(error) = call {
            self.report.check(name, Outcome::Failed, &error.to_string());
            return;
        }
        match self.person.seen() {
            None => self
                .report
                .check(name, Outcome::Noted, "sent, no window in a dry run"),
            Some(seen) => self
                .report
                .check(name, passed_if(ok(&seen)), &pad::summary(&seen)),
        }
    }

    pub fn identity(&mut self) {
        let snapshot = self.device.snapshot();
        self.report.line(&format!("{:?}", snapshot.info));
        for (action, capability) in snapshot.capabilities.iter() {
            self.report.line(&format!(
                "  {action}: {:?}{}",
                capability.execution,
                if capability.quirks.is_empty() {
                    String::new()
                } else {
                    format!(" {:?}", capability.quirks)
                }
            ));
        }
    }
}
pub fn close(device: &Device, person: &Person, report: &mut Report) {
    let failed = report.failed();
    let before = person.seen();
    if let Some(ui) = &person.ui {
        ui.change(|v| {
            v.phase = crate::person::Phase::CleaningUp;
            v.text = "letting go and saving the report".into();
        });
    }
    match device.close() {
        Ok(()) => report.check("close", Outcome::Passed, ""),
        Err(error) => {
            if let Some(ui) = &person.ui {
                ui.change(|v| v.cleanup_failed = true);
            }
            report.check("close", Outcome::Failed, &error.to_string());
            person.say(&format!(
                "closing the box didn't fully work ({error}), so if a button or key is still held or locked, \
                 unplug the box and plug it back in"
            ));
        }
    }
    std::thread::sleep(person.scaled(Duration::from_secs(1)));
    match (before, person.seen()) {
        (Some(before), Some(all)) => {
            report.line(&format!("while closing {}", pad::summary(&all)));
            let known_down = pad::ends_down(&before, Button::Middle) == Some(true);
            report.check(
                "close released held middle",
                if known_down {
                    passed_if(pad::ends_down(&all, Button::Middle) == Some(false))
                } else {
                    Outcome::Noted
                },
                if known_down {
                    "windows release after known down"
                } else {
                    "middle wasn't confirmed down"
                },
            );
            if person.keyboard() && pad::key_down(&before, "F13") {
                report.check(
                    "close released held F13",
                    passed_if(pad::key_pair(&all, "F13")),
                    "windows key edges",
                );
            }
        }
        _ => report.check(
            "close released held middle",
            Outcome::Noted,
            "no window in a dry run",
        ),
    }
    if let Some(ui) = &person.ui {
        ui.change(|v| {
            if let Some(card) = v.cards.iter_mut().find(|c| c.title == "close") {
                card.result = if report.failed() > failed {
                    "failed"
                } else {
                    "recorded"
                }
                .into();
            }
        });
    }
}
pub fn closing<T>(
    device: &Device,
    person: &Person,
    report: &mut Report,
    checks: impl FnOnce(&mut Report) -> Run<T>,
) -> Run<T> {
    let ran = checks(report);
    close(device, person, report);
    ran
}
