use super::*;

impl Checks<'_> {
    pub(super) fn forced_release(&mut self, release_only: bool) -> Run<()> {
        let name = if release_only {
            "left held by you while anybox releases it"
        } else {
            "left held by you while anybox presses and releases it"
        };
        if !self.person.ready(if release_only {
            "hold left while anybox tries to let go of it"
        } else {
            "hold left while anybox presses and releases it"
        })? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        let Some(mut log) =
            self.acquire(&events, "hold left in the box below", |log, window| {
                (self.person.scripted() || pad::ends_down(window, Button::Left) == Some(true))
                    && (!supported(self.device, Action::ButtonEvents)
                        || presses_of(log, Button::Left) > 0)
            })?
        else {
            return Ok(());
        };
        let acquired = self.person.scripted()
            || self
                .person
                .seen()
                .is_some_and(|w| pad::ends_down(&w, Button::Left) == Some(true));
        if !acquired {
            self.report.check(
                name,
                Outcome::Noted,
                "we didn't see the hold, retry or skip",
            );
            return Ok(());
        }
        self.person.action(
            "keep holding until this bar is full",
            Duration::from_secs(4),
        );
        collect(self.person, &events, Duration::from_secs(1), &mut log)?;
        let pressed = if release_only {
            Ok(())
        } else {
            self.device.press(Button::Left)
        };
        collect(self.person, &events, Duration::from_millis(500), &mut log)?;
        let released = self.device.release(Button::Left);
        collect(self.person, &events, Duration::from_millis(2500), &mut log)?;
        let before = self.person.seen();
        let release_start = before.as_ref().map_or(0, Vec::len);
        let event_start = log.len();
        self.person.stop();
        self.person.cue("let go now");
        if let Some(ui) = &self.person.ui {
            ui.change(|v| v.phase = Phase::Release);
        }
        collect(self.person, &events, Duration::from_secs(5), &mut log)?;
        let detail = format!(
            "before release cue {}, box events {}",
            before
                .as_ref()
                .map(|w| pad::buttons(w))
                .unwrap_or_else(|| "no window in a dry run".into()),
            tally(&log)
        );
        match pressed.and(released) {
            Err(error) => self.report.check(name, Outcome::Failed, &error.to_string()),
            Ok(()) => self.report.check(name, Outcome::Noted, &detail),
        }
        if let Some(seen) = self.person.seen() {
            let released_after_cue =
                pad::releases(&seen[release_start.min(seen.len())..], Button::Left) > 0;
            let physical_up = log[event_start..].iter().any(|e| {
                matches!(
                    e,
                    Event::Button {
                        button: Button::Left,
                        pressed: false,
                        ..
                    }
                )
            });
            self.report.check(
                "physical hold released after cue",
                passed_if(
                    released_after_cue
                        && pad::ends_down(&seen, Button::Left) == Some(false)
                        && (!supported(self.device, Action::ButtonEvents) || physical_up),
                ),
                &pad::buttons(&seen),
            );
        }
        Ok(())
    }
    pub(super) fn own_output_in_events(&mut self) -> Run<()> {
        if !self.person.ready("click left while anybox clicks middle")? {
            self.report
                .check("injection in events", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        self.person
            .action("click left a few times now", Duration::from_secs(10));
        let mut log = Vec::new();
        let mut sent = Ok(());
        for _ in 0..5 {
            sent = sent.and(self.device.click(Button::Middle));
            collect(self.person, &events, Duration::from_secs(2), &mut log)?;
        }
        self.person.stop();
        self.report.check(
            "whether events show what anybox sends",
            if sent.is_ok() {
                Outcome::Noted
            } else {
                Outcome::Failed
            },
            &format!(
                "sent {sent:?}, middle presses {}, {}",
                presses_of(&log, Button::Middle),
                tally(&log)
            ),
        );
        self.window("injected middle while clicking left", sent, |seen| {
            window_pair(seen, Button::Middle)
        });
        Ok(())
    }
}
