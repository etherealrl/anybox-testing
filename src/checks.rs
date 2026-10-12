use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anybox::{
    Action, Axis, Button, Click, Device, Direction, Error, Event, EventKinds, Events, Key, Quirk,
};

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

/// everything that came in so far, without waiting
fn drain(events: &Events) -> Vec<Event> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

/// what came in, counted by kind, so a report line stays short even after a lot of movement
fn tally(events: &[Event]) -> String {
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

/// a y/n answer into the report
fn asked(report: &mut Report, name: &str, answer: Option<bool>, detail: &str) {
    let outcome = match answer {
        Some(true) => Outcome::Passed,
        Some(false) => Outcome::Failed,
        None => Outcome::Skipped,
    };
    report.check(name, outcome, detail);
}

impl Checks<'_> {
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

    /// everything that needs nobody, with the mouse parked over the desktop
    pub fn automatic(&mut self) -> Run<()> {
        let person = self.person;
        if !person.ready(
            "next anybox moves your cursor in a small square, scrolls one notch each way and clicks right, middle and \
             the side buttons. put your cursor over an empty spot of your desktop (NOT over this window, a right \
             click in here pastes) and don't touch the mouse until it says done",
        )? {
            self.report.check("automatic part", Outcome::Skipped, "");
            return Ok(());
        }
        person.wait(Duration::from_secs(3))?;

        let mut times = Vec::new();
        let mut moved = Ok(());
        for (dx, dy) in [(100, 0), (0, 100), (-100, 0), (0, -100)] {
            let at = Instant::now();
            moved = moved.and(self.device.move_by(dx, dy));
            times.push(format!("{:?}", at.elapsed()));
            person.wait(Duration::from_millis(400))?;
        }
        let square = format!("each move took {}", times.join(", "));
        result(self.report, "square of moves", moved, &square);

        result(self.report, "zero move", self.device.move_by(0, 0), "");
        let boundary = self
            .device
            .move_by(i16::MAX, i16::MAX)
            .and(self.device.move_by(-i16::MAX, -i16::MAX));
        result(
            self.report,
            "i16 boundary moves",
            boundary,
            "32767 each way then back, the cursor may have hit the screen edge",
        );
        person.wait(Duration::from_millis(400))?;

        for notches in [1, -1, 0] {
            result(
                self.report,
                &format!("wheel {notches}"),
                self.device.wheel(notches),
                "",
            );
            person.wait(Duration::from_millis(300))?;
        }
        let limit = quirks(self.device, Action::Wheel)
            .iter()
            .find_map(|q| match q {
                Quirk::NotchesPerCommand(n) => Some(*n),
                _ => None,
            });
        if limit == Some(1) {
            match self.device.wheel(2) {
                Err(Error::Unsupported { .. }) | Err(Error::InvalidArgument { .. }) => self
                    .report
                    .check("wheel 2 refused before sending", Outcome::Passed, ""),
                other => self.report.check(
                    "wheel 2 refused before sending",
                    Outcome::Failed,
                    &format!("{other:?}"),
                ),
            }
        }

        for button in [Button::Right, Button::Middle, Button::Side1, Button::Side2] {
            let pressed = self.device.press(button);
            person.wait(Duration::from_millis(60))?;
            let released = self.device.release(button);
            result(
                self.report,
                &format!("press and release {button:?}"),
                pressed.and(released),
                "",
            );
            person.wait(Duration::from_millis(300))?;
        }
        let at = Instant::now();
        let clicked = self
            .device
            .click_with(Button::Right, Click::hold(Duration::from_millis(80)));
        let took = format!("asked for an 80 ms hold, the call took {:?}", at.elapsed());
        result(self.report, "right click with a host hold", clicked, &took);
        let repeated = (0..5).try_for_each(|_| self.device.click(Button::Middle));
        result(self.report, "five middle clicks in a row", repeated, "");

        self.refusals();
        self.keys()?;
        person.say("done, you can use your mouse again (press Escape if a menu opened)");
        Ok(())
    }

    /// locks the core should refuse before anything goes out, and the ones a box may or may not take
    fn refusals(&mut self) {
        if quirks(self.device, Action::LockAxis).contains(&Quirk::WholeAxisOnly) {
            match self.device.lock_axis(Axis::X, Direction::Positive) {
                Err(Error::Unsupported { .. }) => {
                    self.report
                        .check("one way X lock refused", Outcome::Passed, "")
                }
                other => {
                    let _ = self.device.unlock_axis(Axis::X, Direction::Both);
                    self.report.check(
                        "one way X lock refused",
                        Outcome::Failed,
                        &format!("{other:?}"),
                    )
                }
            }
        }
        for (name, axis) in [("wheel lock", Axis::Wheel), ("pan lock", Axis::Pan)] {
            match self.device.lock_axis(axis, Direction::Both) {
                Ok(()) => {
                    let undone = self.device.unlock_axis(axis, Direction::Both);
                    self.report.check(
                        name,
                        Outcome::Noted,
                        &format!("the box took it, unlocking it again: {undone:?}"),
                    )
                }
                Err(error) => self.report.check(name, Outcome::Noted, &error.to_string()),
            }
        }
        if !supported(self.device, Action::LockKey) {
            match self.device.lock_key(F13) {
                Err(Error::Unsupported { .. }) => {
                    self.report.check("key lock refused", Outcome::Passed, "")
                }
                other => {
                    let _ = self.device.unlock_key(F13);
                    self.report
                        .check("key lock refused", Outcome::Failed, &format!("{other:?}"))
                }
            }
        }
    }

    fn keys(&mut self) -> Run<()> {
        if !supported(self.device, Action::KeyPress) {
            let reason = format!(
                "{:?}",
                self.device
                    .snapshot()
                    .capabilities
                    .get(Action::KeyPress)
                    .execution
            );
            self.report.check("keys", Outcome::NotAvailable, &reason);
            return Ok(());
        }
        let pressed = self.device.press_key(F13);
        self.person.wait(Duration::from_millis(100))?;
        let released = self.device.release_key(F13);
        result(
            self.report,
            "F13 press and release",
            pressed.and(released),
            "",
        );
        result(self.report, "F14 tap", self.device.tap_key(F14), "");
        let shifted = self
            .device
            .press_key(Key::LEFT_SHIFT)
            .and(self.device.tap_key(F13));
        let unshifted = self.device.release_key(Key::LEFT_SHIFT);
        result(
            self.report,
            "F13 tap with left shift held",
            shifted.and(unshifted),
            "",
        );
        Ok(())
    }

    /// the parts where they do something with the mouse or keyboard behind the box
    pub fn with_them(&mut self) -> Run<()> {
        self.forced_release()?;
        self.every_input()?;
        self.fast_motion()?;
        self.own_output_in_events()?;
        self.key_merge()?;
        self.button_locks()?;
        for axis in [Axis::X, Axis::Y] {
            self.axis_lock(axis)?;
        }
        self.receivers()
    }

    fn forced_release(&mut self) -> Run<()> {
        let name = "left held by you while anybox presses and releases it";
        if !self.person.ready(
            "hold the LEFT button down after the beep and keep holding it until the second beep",
        )? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        self.person.beep();
        self.person.wait(Duration::from_secs(3))?;
        let pressed = self.device.press(Button::Left);
        self.person.wait(Duration::from_millis(500))?;
        let released = self.device.release(Button::Left);
        self.person.wait(Duration::from_secs(2))?;
        self.person.beep();
        self.person.say("you can let go now");
        let seen = tally(&drain(&events));
        result(self.report, name, pressed.and(released), &seen);
        let stayed = self.person.yes_no(
            "while you kept holding, was left still down at the end (it can come up for a moment when anybox lets go)?",
        )?;
        asked(
            self.report,
            "left stayed down after anybox let go",
            stayed,
            "",
        );
        Ok(())
    }

    fn every_input(&mut self) -> Run<()> {
        let name = "every kind of input shows up as events";
        if !self.person.ready(
            "after the beep click every button on your mouse, move it, scroll, and press a few keys on the keyboard \
             behind the box if there is one",
        )? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events();
        self.person.timed("go", Duration::from_secs(15))?;
        let seen = drain(&events);
        let buttons = seen.iter().any(|e| matches!(e, Event::Button { .. }));
        let motion = seen.iter().any(|e| matches!(e, Event::Motion { .. }));
        let outcome = if buttons && motion {
            Outcome::Passed
        } else {
            Outcome::Failed
        };
        self.report.check(name, outcome, &tally(&seen));
        Ok(())
    }

    fn fast_motion(&mut self) -> Run<()> {
        let name = "fast motion, events per second and losses";
        if !self
            .person
            .ready("after the beep move your mouse around fast in circles until the second beep")?
        {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::MOTION);
        let wait = Duration::from_secs(5);
        self.person.timed("go", wait)?;
        let seen = drain(&events);
        let motion = seen
            .iter()
            .filter(|e| matches!(e, Event::Motion { .. }))
            .count();
        let lost: u64 = seen
            .iter()
            .map(|e| match e {
                Event::Dropped { count, .. } => *count,
                _ => 0,
            })
            .sum();
        let rate = motion as f64 / self.person.scaled(wait).as_secs_f64();
        let detail = format!("{motion} motion events, about {rate:.0} a second, {lost} lost");
        self.report.check(name, Outcome::Noted, &detail);
        Ok(())
    }

    fn own_output_in_events(&mut self) -> Run<()> {
        let name = "whether events show what anybox sends";
        if !self.person.ready(
            "for 10 seconds after the beep, click LEFT a few times with your cursor over the desktop. anybox clicks \
             middle every 2 seconds at the same time",
        )? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        self.person.beep();
        let until = Instant::now() + self.person.scaled(Duration::from_secs(10));
        let mut clicks = 0;
        let mut sent = Ok(());
        while Instant::now() < until {
            sent = sent.and(self.device.click(Button::Middle));
            clicks += 1;
            self.person.wait(Duration::from_secs(2))?;
        }
        self.person.beep();
        let seen = drain(&events);
        let middle = seen
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Button {
                        button: Button::Middle,
                        pressed: true,
                        ..
                    }
                )
            })
            .count();
        let detail = format!(
            "anybox clicked middle {clicks} times and {middle} middle presses came back as events, all events: {}",
            tally(&seen)
        );
        match sent {
            Ok(()) => self.report.check(name, Outcome::Noted, &detail),
            Err(error) => self
                .report
                .check(name, Outcome::Failed, &format!("{error}, {detail}")),
        }
        Ok(())
    }

    fn key_merge(&mut self) -> Run<()> {
        let name = "a key held by anybox merges with your typing";
        if !supported(self.device, Action::KeyPress) {
            self.report.check(name, Outcome::NotAvailable, "no keys");
            return Ok(());
        }
        let typed = self
            .person
            .yes_no("is there a keyboard plugged into the box?")?;
        if typed != Some(true) {
            self.report
                .check(name, Outcome::NotAvailable, "no keyboard behind the box");
            return Ok(());
        }
        let held = self.device.press_key(Key::LEFT_SHIFT);
        let line = self.person.answer(
            "anybox holds shift now, so type a few letters on the keyboard behind the box and press Enter:",
            "ABC",
        );
        let released = self.device.release_key(Key::LEFT_SHIFT);
        let line = line?;
        let upper = !line.is_empty() && line.chars().all(|c| !c.is_ascii_lowercase());
        match held.and(released) {
            Err(error) => self.report.check(name, Outcome::Failed, &error.to_string()),
            Ok(()) => self.report.check(
                name,
                if upper {
                    Outcome::Passed
                } else {
                    Outcome::Failed
                },
                &format!("typed {line:?}"),
            ),
        }
        Ok(())
    }

    fn button_locks(&mut self) -> Run<()> {
        let name = "all five buttons locked";
        if !supported(self.device, Action::LockButton) {
            self.report.check(name, Outcome::NotAvailable, "");
            return Ok(());
        }
        if !self.person.ready(
            "next anybox locks every mouse button for 8 seconds. after the beep click each button a couple times \
             with your cursor over the desktop, nothing should happen on screen",
        )? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        let locked: anybox::Result<()> =
            BUTTONS.iter().try_for_each(|b| self.device.lock_button(*b));
        let waited = self.person.timed("go", Duration::from_secs(8));
        // every unlock gets tried even after one fails, a button left locked is the worst outcome here
        let unlocks: Vec<anybox::Result<()>> = BUTTONS
            .iter()
            .map(|b| self.device.unlock_button(*b))
            .collect();
        let unlocked: anybox::Result<()> = unlocks.into_iter().collect();
        waited?;
        let seen = tally(&drain(&events));
        if let Err(error) = locked.and(unlocked) {
            self.report.check(name, Outcome::Failed, &error.to_string());
            return Ok(());
        }
        let blocked = self
            .person
            .yes_no("did your clicks do nothing on screen?")?;
        asked(
            self.report,
            name,
            blocked,
            &format!("events while locked: {seen}"),
        );
        Ok(())
    }

    fn axis_lock(&mut self, axis: Axis) -> Run<()> {
        let name = format!("{axis} locked");
        if !supported(self.device, Action::LockAxis) {
            self.report.check(&name, Outcome::NotAvailable, "");
            return Ok(());
        }
        let (along, across) = match axis {
            Axis::X => ("sideways", "up and down"),
            _ => ("up and down", "sideways"),
        };
        if !self.person.ready(&format!(
            "next anybox locks your mouse {along} for 8 seconds and pushes the cursor {along} itself a few times. \
             after the beep move your mouse around in every direction"
        ))? {
            self.report.check(&name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::MOTION);
        let locked = self.device.lock_axis(axis, Direction::Both);
        self.person.beep();
        let mut pushed = Ok(());
        for step in 0..4 {
            self.person.wait(Duration::from_secs(2))?;
            let amount = if step % 2 == 0 { 150 } else { -150 };
            let (dx, dy) = if axis == Axis::X {
                (amount, 0)
            } else {
                (0, amount)
            };
            pushed = pushed.and(self.device.move_by(dx, dy));
        }
        let unlocked = self.device.unlock_axis(axis, Direction::Both);
        self.person.beep();
        let seen = tally(&drain(&events));
        if let Err(error) = locked.and(pushed).and(unlocked) {
            self.report
                .check(&name, Outcome::Failed, &error.to_string());
            return Ok(());
        }
        let blocked = self.person.yes_no(&format!(
            "could you only move {across}, while anybox still pushed the cursor {along}?"
        ))?;
        asked(
            self.report,
            &name,
            blocked,
            &format!("your motion while locked still came in as: {seen}"),
        );
        Ok(())
    }

    /// a second receiver only gets its own kinds, and dropping the last one switches the box's events off, which
    /// only the trace shows
    fn receivers(&mut self) -> Run<()> {
        let name = "a buttons only receiver next to an everything receiver";
        if !self
            .person
            .ready("for 5 seconds after the beep, click and move your mouse")?
        {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let all = self.device.events();
        let buttons = self.device.events_of(EventKinds::BUTTONS);
        self.person.timed("go", Duration::from_secs(5))?;
        let only_buttons = drain(&buttons)
            .iter()
            .all(|e| matches!(e, Event::Button { .. } | Event::Dropped { .. }));
        let everything = tally(&drain(&all));
        self.report.check(
            name,
            if only_buttons {
                Outcome::Passed
            } else {
                Outcome::Failed
            },
            &format!("the everything receiver got: {everything}"),
        );
        drop((all, buttons));
        self.person.wait(Duration::from_secs(1))?;
        let again = self.device.events();
        self.person.wait(Duration::from_secs(1))?;
        drop(again);
        self.report.check(
            "last receiver dropped, then a new one",
            Outcome::Noted,
            "the trace around this time shows whether the box's events went off and on again",
        );
        Ok(())
    }

    /// one unplug: a held button and a lock going through it. `unplug` is what the dry run does instead of a person
    pub fn reconnect(&mut self, what: &str, unplug: Option<&dyn Fn()>) -> Run<()> {
        let name = "unplug and replug";
        if !self.person.ready(&format!(
            "last one for this box. anybox holds right and locks left, then you unplug {what}, wait 3 seconds and \
             plug it back into the SAME USB port"
        ))? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events();
        let held = self.device.press(Button::Right);
        let locked = supported(self.device, Action::LockButton)
            .then(|| self.device.lock_button(Button::Left));
        self.person.say(&format!("unplug {what} now"));
        if let Some(unplug) = unplug {
            unplug();
        }
        let mut log = Vec::new();
        let came_back =
            self.await_event(&events, &mut log, |e| matches!(e, Event::Disconnected))? && {
                self.person
                    .say("now plug it back in (anybox waits up to 60 seconds)");
                self.await_event(&events, &mut log, |e| matches!(e, Event::Reconnected))?
            };
        let detail = format!(
            "press {held:?}, lock {locked:?}, then {}",
            if log.is_empty() {
                "nothing".to_string()
            } else {
                log.join(", ")
            }
        );
        if !came_back {
            self.report.check(name, Outcome::Failed, &detail);
            self.person
                .say("it didn't come back, that's fine, it's in the report and the run carries on");
            let _ = self.device.release(Button::Right);
            let _ = self.device.unlock_button(Button::Left);
            return Ok(());
        }
        self.report.check(name, Outcome::Passed, &detail);
        self.report.line(&format!(
            "after it came back: {:?}",
            self.device.snapshot().info
        ));
        if locked.is_some() {
            let still = self.person.yes_no(
                "click left a couple times over the desktop. does left still do nothing?",
            )?;
            asked(
                self.report,
                "left lock put back after the reconnect",
                still,
                "",
            );
            result(
                self.report,
                "unlock left after the reconnect",
                self.device.unlock_button(Button::Left),
                "",
            );
        }
        let up = self
            .person
            .yes_no("click right over the desktop. does it work like normal (not stuck held)?")?;
        asked(self.report, "right let go after the reconnect", up, "");
        let after = self.device.events_of(EventKinds::BUTTONS);
        self.person
            .timed("click any button a few times", Duration::from_secs(5))?;
        let seen = drain(&after);
        self.report.check(
            "events after the reconnect",
            if seen.is_empty() {
                Outcome::Failed
            } else {
                Outcome::Passed
            },
            &tally(&seen),
        );
        Ok(())
    }

    fn await_event(
        &self,
        events: &Events,
        log: &mut Vec<String>,
        wanted: impl Fn(&Event) -> bool,
    ) -> Run<bool> {
        let until = Instant::now()
            + self
                .person
                .scaled(Duration::from_secs(60))
                .max(Duration::from_secs(5));
        while Instant::now() < until {
            self.person.wait(Duration::from_millis(1))?;
            if let Ok(event) = events.recv_timeout(Duration::from_millis(100)) {
                if !matches!(event, Event::Motion { .. } | Event::Button { .. }) {
                    log.push(format!("{event:?}"));
                }
                if wanted(&event) {
                    return Ok(true);
                }
            }
        }
        log.push("gave up after 60 seconds".to_string());
        Ok(false)
    }
}

/// closes it and says what close couldn't do, because a failed close can leave something held
pub fn close(device: &Device, person: &Person, report: &mut Report) {
    match device.close() {
        Ok(()) => report.check("close", Outcome::Passed, ""),
        Err(error) => {
            report.check("close", Outcome::Failed, &error.to_string());
            person.say(&format!(
                "closing the box didn't fully work ({error}), so if a button or key is still held or locked, \
                 unplug the box and plug it back in"
            ));
        }
    }
}
