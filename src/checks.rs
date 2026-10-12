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
pub fn drain(events: &Events) -> Vec<Event> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

/// a timed part. it keeps reading the whole time, because the queue only holds so much and a fast mouse fills it
/// in a second or two
pub fn gather(person: &Person, events: &Events, text: &str, wait: Duration) -> Run<Vec<Event>> {
    person.say(format!("{text} ({} seconds)", wait.as_secs()).trim_start());
    person.go();
    let mut seen = Vec::new();
    collect(person, events, wait, &mut seen)?;
    person.stop();
    Ok(seen)
}

/// reads everything that comes in for this long
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

fn buttons_in(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::Button { .. }))
        .count()
}

/// what came in, counted by kind, so a report line stays short even after a lot of movement
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

/// a y/n answer into the report
fn asked(report: &mut Report, name: &str, answer: Option<bool>, detail: &str) {
    let outcome = match answer {
        Some(true) => Outcome::Passed,
        Some(false) => Outcome::Failed,
        None => Outcome::Skipped,
    };
    report.check(name, outcome, detail);
}

fn passed_if(ok: bool) -> Outcome {
    if ok { Outcome::Passed } else { Outcome::Failed }
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

        // last, since the cursor ends up against a screen edge and nothing may click after it
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
        let events = self.device.events();
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
        // a box that refuses keys only says so after the call returned
        self.person.wait(Duration::from_secs(1))?;
        let seen = drain(&events);
        let refused = seen.iter().any(|e| matches!(e, Event::Rejected { .. }));
        self.report.check(
            "no key refused afterwards",
            passed_if(!refused),
            &tally(&seen),
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
            "when it says NOW (with a beep), hold the LEFT button down and keep holding it until it says done, with your cursor \
             over the desktop",
        )? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        self.person.go();
        self.person.wait(Duration::from_secs(3))?;
        let pressed = self.device.press(Button::Left);
        self.person.wait(Duration::from_millis(500))?;
        let released = self.device.release(Button::Left);
        self.person.wait(Duration::from_secs(2))?;
        self.person.stop();
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
        if !self.person.ready(
            "when it says NOW, click EVERY button on your mouse, move it, scroll up and down, and press a few keys on \
             the keyboard behind the box if there is one",
        )? {
            self.report
                .check("every kind of input", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events();
        let seen = gather(self.person, &events, "", Duration::from_secs(10))?;
        self.report
            .line(&format!("everything that came in: {}", tally(&seen)));
        let pressed: Vec<Button> = seen
            .iter()
            .filter_map(|e| match e {
                Event::Button {
                    button,
                    pressed: true,
                    ..
                } => Some(*button),
                _ => None,
            })
            .collect();
        let snapshot = self.device.snapshot();
        let capabilities = &snapshot.capabilities;
        let each: [(&str, Action, bool); 4] = [
            (
                "button events, all five buttons",
                Action::ButtonEvents,
                BUTTONS.iter().all(|b| pressed.contains(b)),
            ),
            (
                "motion events",
                Action::MotionEvents,
                seen.iter()
                    .any(|e| matches!(e, Event::Motion { dx, dy, .. } if *dx != 0 || *dy != 0)),
            ),
            (
                "wheel events",
                Action::MotionEvents,
                seen.iter()
                    .any(|e| matches!(e, Event::Motion { wheel, .. } if *wheel != 0)),
            ),
            (
                "key events",
                Action::KeyEvents,
                seen.iter().any(|e| matches!(e, Event::Key { .. })),
            ),
        ];
        for (name, action, ok) in each {
            let capability = capabilities.get(action);
            if capability.is_supported() {
                self.report.check(name, passed_if(ok), "");
            } else {
                self.report.check(
                    name,
                    Outcome::NotAvailable,
                    &format!("{:?}", capability.execution),
                );
            }
        }
        Ok(())
    }

    fn fast_motion(&mut self) -> Run<()> {
        let name = "fast motion, events per second and losses";
        if !supported(self.device, Action::MotionEvents) {
            self.report.check(name, Outcome::NotAvailable, "");
            return Ok(());
        }
        if !self
            .person
            .ready("when it says NOW, move your mouse around fast in circles until it says done")?
        {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::MOTION);
        let wait = Duration::from_secs(5);
        let seen = gather(self.person, &events, "", wait)?;
        let motion = seen
            .iter()
            .filter(|e| matches!(e, Event::Motion { .. }))
            .count();
        let rate = motion as f64 / self.person.scaled(wait).as_secs_f64();
        let detail = format!("about {rate:.0} motion events a second, {}", tally(&seen));
        self.report.check(name, Outcome::Noted, &detail);
        Ok(())
    }

    fn own_output_in_events(&mut self) -> Run<()> {
        let name = "whether events show what anybox sends";
        if !supported(self.device, Action::ButtonEvents) {
            self.report.check(name, Outcome::NotAvailable, "");
            return Ok(());
        }
        if !self.person.ready(
            "for 10 seconds after it says NOW, click LEFT a few times with your cursor over the desktop. anybox clicks \
             middle every 2 seconds at the same time",
        )? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        self.person.go();
        let until = Instant::now() + self.person.scaled(Duration::from_secs(10));
        let mut clicks = 0;
        let mut sent = Ok(());
        while Instant::now() < until {
            sent = sent.and(self.device.click(Button::Middle));
            clicks += 1;
            self.person.wait(Duration::from_secs(2))?;
        }
        self.person.stop();
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

    /// letters typed on the keyboard behind the box, while anybox holds shift. F13 would be the safer key but
    /// hardly any keyboard has one
    fn key_merge(&mut self) -> Run<()> {
        let name = "a key held by anybox merges with your typing";
        if !supported(self.device, Action::KeyPress) {
            self.report.check(name, Outcome::NotAvailable, "no keys");
            return Ok(());
        }
        let keyboard = self
            .person
            .yes_no("is there a keyboard plugged into the box?")?;
        if keyboard != Some(true) {
            self.report
                .check(name, Outcome::NotAvailable, "no keyboard behind the box");
            return Ok(());
        }
        let held = self.device.press_key(Key::LEFT_SHIFT);
        let line = self.person.answer(
            "anybox holds shift now, so type abc on the keyboard behind the box and press Enter",
            "in lowercase with caps lock off, it should come out as ABC",
            "ABC",
        );
        let released = self.device.release_key(Key::LEFT_SHIFT);
        let line = line?;
        let letters: Vec<char> = line.chars().filter(char::is_ascii_alphabetic).collect();
        let upper = !letters.is_empty() && letters.iter().all(char::is_ascii_uppercase);
        match held.and(released) {
            Err(error) => self.report.check(name, Outcome::Failed, &error.to_string()),
            Ok(()) => self.report.check(
                name,
                passed_if(upper),
                &format!("typed {} letters, all uppercase {upper}", letters.len()),
            ),
        }
        Ok(())
    }

    fn button_locks(&mut self) -> Run<()> {
        if !supported(self.device, Action::LockButton) {
            self.report.check("button locks", Outcome::NotAvailable, "");
            return Ok(());
        }
        if !self.person.ready(
            "next anybox locks your mouse buttons one at a time for 3 seconds each, it tells you which one. click \
             that button a couple times with your cursor over the desktop, nothing should happen on screen",
        )? {
            self.report.check("button locks", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        for button in BUTTONS {
            let locked = self.device.lock_button(button);
            let seen = gather(
                self.person,
                &events,
                &format!("{button:?} is locked, click it"),
                Duration::from_secs(3),
            );
            let unlocked = self.device.unlock_button(button);
            let seen = seen?;
            match locked.and(unlocked) {
                Err(error) => self.report.check(
                    &format!("{button:?} locked"),
                    Outcome::Failed,
                    &error.to_string(),
                ),
                Ok(()) => self.report.check(
                    &format!("{button:?} locked, your clicks still came in as events"),
                    // only a press of this button counts, a release can be late from the one before
                    if seen.iter().any(|e| {
                        matches!(e, Event::Button { button: b, pressed: true, .. } if *b == button)
                    }) {
                        Outcome::Passed
                    } else {
                        Outcome::Noted
                    },
                    &tally(&seen),
                ),
            }
        }
        let blocked = self
            .person
            .yes_no("did every button do nothing on screen while it was locked?")?;
        asked(self.report, "button locks block your clicks", blocked, "");

        // software output through a lock, right shows it best because a menu opens
        if !self.person.ready(
            "now right gets locked and anybox clicks right once itself. put your cursor over the desktop",
        )? {
            self.report
                .check("anybox's own click goes through a lock", Outcome::Skipped, "");
            return Ok(());
        }
        let locked = self.device.lock_button(Button::Right);
        self.person.wait(Duration::from_millis(300))?;
        let clicked = self.device.click(Button::Right);
        self.person.wait(Duration::from_millis(500))?;
        let unlocked = self.device.unlock_button(Button::Right);
        if let Err(error) = locked.and(clicked).and(unlocked) {
            self.report.check(
                "anybox's own click goes through a lock",
                Outcome::Failed,
                &error.to_string(),
            );
            return Ok(());
        }
        let opened = self
            .person
            .yes_no("did a right click menu open (press Escape to close it)?")?;
        asked(
            self.report,
            "anybox's own click goes through a lock",
            opened,
            "",
        );
        Ok(())
    }

    fn axis_lock(&mut self, axis: Axis) -> Run<()> {
        let name = format!("{axis} locked");
        if !supported(self.device, Action::LockAxis) {
            self.report.check(&name, Outcome::NotAvailable, "");
            return Ok(());
        }
        let one_way = axis == Axis::X
            && !quirks(self.device, Action::LockAxis).contains(&Quirk::WholeAxisOnly);
        let (along, across) = match axis {
            Axis::X => ("sideways", "up and down"),
            _ => ("up and down", "sideways"),
        };
        let text = if one_way {
            "next anybox locks your mouse one way at a time for 8 seconds, first you can only go left, then only \
             right, and it pushes the cursor sideways itself a few times. when it says NOW, move your mouse around in \
             every direction"
                .to_string()
        } else {
            format!(
                "next anybox locks your mouse {along} for 8 seconds and pushes the cursor {along} itself a few times. \
                 when it says NOW, move your mouse around in every direction"
            )
        };
        if !self.person.ready(&text)? {
            self.report.check(&name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::MOTION);
        let first = if one_way {
            Direction::Positive
        } else {
            Direction::Both
        };
        let mut done = self.device.lock_axis(axis, first);
        self.person.go();
        let mut seen = Vec::new();
        for step in 0..4 {
            if one_way && step == 2 {
                done = done
                    .and(self.device.unlock_axis(axis, Direction::Positive))
                    .and(self.device.lock_axis(axis, Direction::Negative));
                self.person.say("now you can only go right");
            }
            collect(self.person, &events, Duration::from_secs(2), &mut seen)?;
            let amount = if step % 2 == 0 { 150 } else { -150 };
            let (dx, dy) = if axis == Axis::X {
                (amount, 0)
            } else {
                (0, amount)
            };
            done = done.and(self.device.move_by(dx, dy));
        }
        let last = if one_way {
            Direction::Negative
        } else {
            Direction::Both
        };
        let unlocked = self.device.unlock_axis(axis, last);
        self.person.stop();
        seen.extend(drain(&events));
        if let Err(error) = done.and(unlocked) {
            self.report
                .check(&name, Outcome::Failed, &error.to_string());
            return Ok(());
        }
        let question = if one_way {
            "could you only go left for the first half and only right for the second, while anybox still pushed \
             the cursor both ways?"
                .to_string()
        } else {
            format!("could you only move {across}, while anybox still pushed the cursor {along}?")
        };
        let blocked = self.person.yes_no(&question)?;
        let name = if one_way {
            format!("{axis} locked one way at a time")
        } else {
            name
        };
        asked(
            self.report,
            &name,
            blocked,
            &format!(
                "your motion while locked still came in as: {}",
                tally(&seen)
            ),
        );
        Ok(())
    }

    /// a second receiver only gets its own kinds, and dropping the last one switches the box's events off, which
    /// only the trace shows
    fn receivers(&mut self) -> Run<()> {
        let name = "a buttons only receiver next to an everything receiver";
        if !supported(self.device, Action::ButtonEvents) {
            self.report.check(name, Outcome::NotAvailable, "");
            return Ok(());
        }
        if !self
            .person
            .ready("for 5 seconds after it says NOW, click and move your mouse")?
        {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let buttons = self.device.events_of(EventKinds::BUTTONS);
        let all = self.device.events();
        let everything = gather(self.person, &all, "", Duration::from_secs(5))?;
        let only = drain(&buttons);
        let input_only_buttons = only.iter().all(|e| {
            !matches!(
                e,
                Event::Motion { .. } | Event::Key { .. } | Event::Media { .. }
            )
        });
        self.report.check(
            name,
            passed_if(input_only_buttons && buttons_in(&only) > 0),
            &format!(
                "the buttons receiver got: {}, the everything receiver got: {}",
                tally(&only),
                tally(&everything)
            ),
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
        // a box that keeps held buttons through a reconnect is allowed to, so it's only noted there
        let keeps = quirks(self.device, Action::Press).contains(&Quirk::KeepsHeld);
        let up = self.person.yes_no(
            "is right let go now (a right click over the desktop works like normal, not stuck held)?",
        )?;
        if keeps {
            self.report.check(
                "right after the reconnect",
                Outcome::Noted,
                &format!("the box keeps held buttons, let go: {up:?}"),
            );
        } else {
            asked(self.report, "right let go after the reconnect", up, "");
        }
        let _ = self.device.release(Button::Right);
        let after = self.device.events_of(EventKinds::BUTTONS);
        let seen = gather(
            self.person,
            &after,
            "click any button a few times",
            Duration::from_secs(5),
        )?;
        self.report.check(
            "button events after the reconnect",
            passed_if(buttons_in(&seen) > 0),
            &tally(&seen),
        );
        Ok(())
    }

    /// the last thing before close, so close has something held to let go of, and the trace shows it does
    pub fn hold_for_close(&mut self) -> Run<()> {
        let name = "middle held when the box gets closed";
        if !self.person.ready(
            "last bit: anybox holds middle and then closes the box, which should let go of it. put your cursor over              the desktop",
        )? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        match self.device.press(Button::Middle) {
            Ok(()) => self.report.check(
                name,
                Outcome::Noted,
                "the trace after this shows what close sent",
            ),
            Err(error) => self.report.check(name, Outcome::Failed, &error.to_string()),
        }
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
            self.person.stopped()?;
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

/// runs `checks` on a device and always closes it afterwards, a stopped run included
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
