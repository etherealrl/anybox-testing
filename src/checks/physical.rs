mod buttons;
mod holds;
mod input;
mod keyboard;

use super::*;
use crate::person::{Answer, Phase};

fn button_pair(events: &[Event], button: Button) -> bool {
    presses_of(events, button) > 0
        && events
            .iter()
            .any(|e| matches!(e, Event::Button { button: b, pressed: false, .. } if *b == button))
}
fn key_pair(events: &[Event], key: Key) -> bool {
    [true, false].into_iter().all(|down| {
        events.iter().any(
            |e| matches!(e, Event::Key { key: k, pressed, .. } if *k == key && *pressed == down),
        )
    })
}
fn window_pair(seen: &[Seen], button: Button) -> bool {
    pad::presses(seen, button) > 0 && pad::releases(seen, button) > 0
}

impl Checks<'_> {
    pub fn with_them(&mut self) -> Run<()> {
        if self.person.keyboard() {
            for name in [
                "physical buttons",
                "physical movement",
                "physical wheel",
                "fast motion",
                "physical holds",
                "injection in events",
                "button locks",
                "axis locks",
            ] {
                self.report
                    .check(name, Outcome::NotAvailable, "a keyboard is attached");
            }
            self.card("physical keys", Self::physical_keys)?;
            self.card("key merging", Self::key_merge)?;
        } else {
            self.card("physical buttons", Self::physical_buttons)?;
            // moving the mouse around may take the cursor out of the test area, that's fine here
            self.free_cursor(true);
            self.card("physical movement", Self::physical_motion)?;
            self.card("physical wheel", Self::physical_wheel)?;
            self.card("fast movement", Self::fast_motion)?;
            self.free_cursor(false);
            self.card("held button, press and release", |s| {
                s.forced_release(false)
            })?;
            self.card("held button, release only", |s| s.forced_release(true))?;
            self.card("injection in events", Self::own_output_in_events)?;
            self.card("button locks", Self::button_locks)?;
            self.free_cursor(true);
            for axis in [Axis::X, Axis::Y] {
                self.card(&format!("{axis} lock").to_lowercase(), |s| {
                    s.axis_lock(axis)
                })?;
            }
            self.free_cursor(false);
            self.report
                .check("key merging", Outcome::NotAvailable, "a mouse is attached");
        }
        self.card("receiver filtering", Self::receivers)
    }

    pub(super) fn acquire(
        &self,
        events: &Events,
        prompt: &str,
        predicate: impl Fn(&[Event], &[Seen]) -> bool,
    ) -> Run<Option<Vec<Event>>> {
        self.person.pad_clear();
        drain(events);
        self.person.action(prompt, Duration::from_secs(10));
        if prompt.starts_with("hold ")
            && let Some(ui) = &self.person.ui
        {
            ui.change(|v| v.timer = None);
        }
        let until = Instant::now() + self.person.scaled(Duration::from_secs(10));
        let mut log = Vec::new();
        while Instant::now() < until {
            self.person.stopped()?;
            if let Some(ui) = &self.person.ui
                && matches!(ui.poll(), Some(Answer::No))
            {
                return Ok(None);
            }
            collect(self.person, events, Duration::from_millis(50), &mut log)?;
            if predicate(&log, &self.person.seen().unwrap_or_default()) {
                return Ok(Some(log));
            }
        }
        self.person.cue("we didn't see that, try again or skip");
        Ok(Some(log))
    }
}

impl Checks<'_> {
    fn free_cursor(&self, free: bool) {
        if let Some(ui) = &self.person.ui {
            ui.change(|v| v.require_area = !free && v.can_position);
        }
    }
}

pub(super) fn button_name(button: Button) -> &'static str {
    match button {
        Button::Left => "left button",
        Button::Right => "right button",
        Button::Middle => "middle button (press the wheel)",
        // windows calls these back and forward, back is the one closer to your wrist on most mice
        Button::Side1 => "back side button (closer to you)",
        Button::Side2 => "front side button (further from you)",
        _ => "button",
    }
}
