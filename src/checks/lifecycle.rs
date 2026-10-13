use super::*;

impl Checks<'_> {
    pub fn reconnect(&mut self, _what: &str, unplug: Option<&dyn Fn()>) -> Run<()> {
        if let Some(ui) = &self.person.ui {
            ui.change(|v| v.reconnect_failed = false);
        }
        let name = "unplug and replug";
        if !self.person.ready(
            "anybox holds right and locks left, hands off until it tells you to unplug the box",
        )? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events();
        self.person.pad_clear();
        self.person.action(
            "hands off, don't touch anything",
            Duration::from_millis(500),
        );
        // with no pc mouse the cursor can't be kept in the test area, so the click could land anywhere
        let held = if self.person.can_position() {
            self.device.press(Button::Right)
        } else {
            Err(anybox::Error::Closed)
        };
        let mut log = Vec::new();
        collect(self.person, &events, Duration::from_millis(500), &mut log)?;
        let known_down = held.is_ok()
            && self
                .person
                .seen()
                .is_some_and(|w| pad::ends_down(&w, Button::Right) == Some(true));
        self.report.check(
            "right held before reconnect",
            if known_down {
                Outcome::Passed
            } else {
                Outcome::Noted
            },
            if self.person.scripted() {
                "no window in a dry run"
            } else {
                "windows state recorded"
            },
        );
        let locked = supported(self.device, Action::LockButton)
            .then(|| self.device.lock_button(Button::Left));
        self.person.action(
            "unplug the box's cable to this pc now",
            Duration::from_secs(60),
        );
        if let Some(unplug) = unplug {
            unplug();
        }
        let wanted = if self.person.keyboard() {
            Action::KeyPress
        } else {
            Action::Press
        };
        // through the Ferrum app the port stays open with the box unplugged, the app just loses its devices, so the
        // box going and coming back shows up as a change
        let device = self.device;
        let gone = |e: &Event| {
            matches!(e, Event::Disconnected)
                || (matches!(e, Event::Changed) && !supported(device, wanted))
        };
        let back = |e: &Event| {
            matches!(e, Event::Reconnected)
                || (matches!(e, Event::Changed) && supported(device, wanted))
        };
        let disconnected = self.await_event(&events, &mut log, gone)?;
        let returned = if disconnected {
            self.person.cue("keep it unplugged");
            collect(self.person, &events, Duration::from_secs(3), &mut log)?;
            self.person.action(
                "plug it back into the same usb port now",
                Duration::from_secs(60),
            );
            // the mock restarts immediately, so the reconnect may already be in the drained log
            log.iter().any(|e| matches!(e, Event::Reconnected))
                || self.await_event(&events, &mut log, back)?
        } else {
            false
        };
        self.report.check(
            name,
            passed_if(returned),
            &format!("press {held:?}, lock {locked:?}, {}", tally(&log)),
        );
        crate::setup::ports(self.person, self.report, "after reconnect");
        if !returned {
            if let Some(ui) = &self.person.ui {
                ui.change(|v| v.reconnect_failed = true);
            }
            result(
                self.report,
                "release right after reconnect failure",
                self.device.release(Button::Right),
                "",
            );
            if locked.is_some() {
                result(
                    self.report,
                    "unlock left after reconnect failure",
                    self.device.unlock_button(Button::Left),
                    "",
                );
            }
            self.person
                .cue("the box didn't come back, retry reconnect or save the report");
            return Ok(());
        }
        // the box comes back before the device plugged into it does, and until then it can't press or release
        self.person.action(
            "it's back, waiting for your device behind the box. hands off",
            Duration::from_secs(15),
        );
        let until = Instant::now() + self.person.scaled(Duration::from_secs(15));
        while !supported(self.device, wanted) && Instant::now() < until {
            collect(self.person, &events, Duration::from_millis(200), &mut log)?;
        }
        self.identity();
        self.person
            .action("hands off, don't touch anything", Duration::from_secs(2));
        collect(self.person, &events, Duration::from_secs(2), &mut log)?;
        let state = self
            .person
            .seen()
            .and_then(|w| pad::ends_down(&w, Button::Right));
        let keeps = quirks(self.device, Action::Press).contains(&Quirk::KeepsHeld);
        // a box that restarts when unplugged comes back with nothing held and never sends windows the release, so
        // windows still thinking right is down says nothing about anybox. the trace shows what anybox sent
        self.report.check(
            "right after the reconnect",
            Outcome::Noted,
            &format!(
                "known down {known_down}, windows still thinks it's down {}, keeps held {keeps}",
                state == Some(true)
            ),
        );
        result(
            self.report,
            "release right after reconnect",
            self.device.release(Button::Right),
            "",
        );
        if state == Some(true) && self.person.can_position() {
            self.person.pad_clear();
            let cleared = self.acquire(&events, "click right once to clear it", |_, window| {
                pad::releases(window, Button::Right) > 0
            })?;
            self.report.check(
                "right cleared by your click after the reconnect",
                Outcome::Noted,
                &format!("{}", cleared.is_some()),
            );
        }
        if self.person.keyboard() {
            if locked.is_some() {
                result(
                    self.report,
                    "unlock left after reconnect",
                    self.device.unlock_button(Button::Left),
                    "",
                );
            }
            self.report.check(
                "left lock put back after the reconnect",
                Outcome::NotAvailable,
                "a keyboard is attached",
            );
            if !supported(self.device, Action::KeyEvents) {
                self.report.check(
                    "key events after reconnect",
                    Outcome::NotAvailable,
                    "no key stream",
                );
                return Ok(());
            }
            let log = self.acquire(&events, "press \"a\" once now", |log, _| [true, false].into_iter().all(|down| log.iter().any(|e| matches!(e, Event::Key { key: Key(4), pressed, .. } if *pressed == down))))?.unwrap_or_default();
            self.report.check(
                "key events after reconnect",
                passed_if(log.iter().any(|e| matches!(e, Event::Key { .. }))),
                &tally(&log),
            );
        } else {
            if locked.as_ref().is_some_and(|r| r.is_ok()) {
                self.person.pad_clear();
                drain(&events);
                let log = gather(
                    self.person,
                    &events,
                    "click the left button twice now, it should still be locked",
                    Duration::from_secs(6),
                );
                let unlocked = self.device.unlock_button(Button::Left);
                let log = log?;
                let evidence = presses_of(&log, Button::Left) > 0;
                let suppressed = self
                    .person
                    .seen()
                    .map(|w| pad::presses(&w, Button::Left) == 0);
                self.report.check(
                    "left lock put back after the reconnect",
                    if !evidence || suppressed.is_none() {
                        Outcome::Noted
                    } else {
                        passed_if(suppressed == Some(true) && unlocked.is_ok())
                    },
                    &format!("unlock {unlocked:?}, {}", tally(&log)),
                );
            }
            let log = self.acquire(&events, "click left once more", |log, w| (self.person.scripted() || (pad::presses(w, Button::Left) > 0 && pad::releases(w, Button::Left) > 0)) && [true, false].into_iter().all(|down| log.iter().any(|e| matches!(e, Event::Button { button: Button::Left, pressed, .. } if *pressed == down))))?.unwrap_or_default();
            self.report.check(
                "button events after the reconnect",
                if supported(self.device, Action::ButtonEvents) {
                    passed_if(presses_of(&log, Button::Left) > 0)
                } else {
                    Outcome::NotAvailable
                },
                &tally(&log),
            );
            self.window("left delivery after reconnect unlock", Ok(()), |w| {
                pad::presses(w, Button::Left) > 0 && pad::releases(w, Button::Left) > 0
            });
        }
        self.person.stop();
        Ok(())
    }

    pub fn hold_for_close(&mut self) -> Run<()> {
        if let Some(ui) = &self.person.ui {
            ui.change(|v| {
                v.section = "close".into();
                if let Some(card) = v.cards.iter_mut().find(|c| c.title == "close") {
                    card.result = "running".into();
                }
            });
        }
        if !self
            .person
            .ready("anybox holds middle and closes the connection, hands off the whole time")?
        {
            self.report
                .check("middle held when the box gets closed", Outcome::Skipped, "");
            return Ok(());
        }
        self.person.pad_clear();
        self.person
            .action("hands off, don't touch anything", Duration::from_secs(1));
        let call = if self.person.can_position() {
            self.device.press(Button::Middle)
        } else {
            Ok(())
        };
        if self.person.keyboard() && supported(self.device, Action::KeyPress) {
            result(
                self.report,
                "F13 held before close",
                self.device.press_key(F13),
                "",
            );
        }
        if self.person.can_position() {
            result(
                self.report,
                "middle held when the box gets closed",
                call,
                "close observation follows",
            );
        } else {
            self.report.check(
                "middle held when the box gets closed",
                Outcome::NotAvailable,
                "no pc pointing device for positioning",
            );
        }
        self.person.wait(Duration::from_millis(300))
    }

    fn await_event(
        &self,
        events: &Events,
        log: &mut Vec<Event>,
        wanted: impl Fn(&Event) -> bool,
    ) -> Run<bool> {
        let until = Instant::now()
            + self
                .person
                .scaled(Duration::from_secs(60))
                .max(Duration::from_secs(5));
        while Instant::now() < until {
            self.person.stopped()?;
            if let Ok(event) = events.recv_timeout(Duration::from_millis(50)) {
                let found = wanted(&event);
                log.push(event);
                if found {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
