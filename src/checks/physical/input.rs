use super::*;

impl Checks<'_> {
    pub(super) fn physical_buttons(&mut self) -> Run<()> {
        if !self
            .person
            .ready("click the button shown in the test area")?
        {
            self.report.check("physical buttons", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        let mut all = Vec::new();
        for button in BUTTONS {
            let side = matches!(button, Button::Side1 | Button::Side2);
            if let Some(ui) = &self.person.ui {
                ui.change(|v| {
                    v.side_choice = side;
                    v.step += 1;
                });
            }
            let log = self.acquire(
                &events,
                &format!("click the {} once in the box below", button_name(button)),
                |log, window| {
                    (self.person.scripted() || window_pair(window, button))
                        && (!supported(self.device, Action::ButtonEvents)
                            || button_pair(log, button))
                },
            )?;
            if let Some(ui) = &self.person.ui {
                ui.change(|v| v.side_choice = false);
            }
            let name = format!("physical {}", button_name(button));
            let Some(log) = log else {
                if side && let Some(ui) = &self.person.ui {
                    ui.change(|v| v.missing.push(button));
                }
                self.report
                    .check(&name, Outcome::NotAvailable, "this mouse doesn't have it");
                continue;
            };
            if !self.person.scripted()
                && self.person.seen().is_none_or(|w| !window_pair(&w, button))
            {
                self.report.check(
                    &name,
                    Outcome::Noted,
                    "we didn't see that, try again or skip",
                );
                return Ok(());
            }
            if let Some(ui) = &self.person.ui {
                ui.change(|v| {
                    if !v.physical_buttons.contains(&button) {
                        v.physical_buttons.push(button);
                    }
                });
            }
            if supported(self.device, Action::ButtonEvents) {
                self.report.check(
                    &format!("{name} events"),
                    passed_if(button_pair(&log, button)),
                    &tally(&log),
                );
            } else {
                self.report.check(
                    &format!("{name} events"),
                    Outcome::NotAvailable,
                    "no physical button stream",
                );
            }
            self.window(&name, Ok(()), |seen| window_pair(seen, button));
            all.extend(log);
        }
        self.person.stop();
        self.report.check(
            "button events, all five buttons",
            if !supported(self.device, Action::ButtonEvents) {
                Outcome::NotAvailable
            } else if BUTTONS.iter().any(|b| self.person.missing(*b)) {
                Outcome::Noted
            } else {
                passed_if(BUTTONS.iter().all(|b| button_pair(&all, *b)))
            },
            &tally(&all),
        );
        Ok(())
    }

    pub(super) fn physical_motion(&mut self) -> Run<()> {
        if !self
            .person
            .ready("move your mouse around inside the test area")?
        {
            self.report.check("motion events", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::MOTION);
        let log = gather(
            self.person,
            &events,
            "move your mouse now",
            Duration::from_secs(4),
        )?;
        let both = log
            .iter()
            .any(|e| matches!(e, Event::Motion { dx, .. } if *dx != 0))
            && log
                .iter()
                .any(|e| matches!(e, Event::Motion { dy, .. } if *dy != 0));
        self.report.check(
            "motion events",
            if supported(self.device, Action::MotionEvents) {
                passed_if(both)
            } else {
                Outcome::NotAvailable
            },
            &tally(&log),
        );
        if let Some(seen) = self.person.seen() {
            self.report.check(
                "physical windows motion",
                Outcome::Noted,
                &pad::summary(&seen),
            );
        }
        Ok(())
    }
    pub(super) fn physical_wheel(&mut self) -> Run<()> {
        if !self.person.ready("scroll up, then down")? {
            self.report.check("wheel events", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::MOTION);
        for positive in [true, false] {
            self.person.pad_clear();
            drain(&events);
            let log = gather(
                self.person,
                &events,
                if positive {
                    "scroll up now"
                } else {
                    "scroll down now"
                },
                Duration::from_secs(5),
            )?;
            let name = if positive {
                "physical wheel up"
            } else {
                "physical wheel down"
            };
            self.report.check(&format!("{name} events"), if supported(self.device, Action::MotionEvents) { passed_if(log.iter().any(|e| matches!(e, Event::Motion { wheel, .. } if if positive { *wheel > 0 } else { *wheel < 0 }))) } else { Outcome::NotAvailable }, &tally(&log));
            self.window(name, Ok(()), |seen| {
                if positive {
                    pad::wheel(seen) > 0.0
                } else {
                    pad::wheel(seen) < 0.0
                }
            });
        }
        Ok(())
    }
    pub(super) fn fast_motion(&mut self) -> Run<()> {
        if !supported(self.device, Action::MotionEvents) {
            self.report
                .check("fast motion", Outcome::NotAvailable, "no motion stream");
            return Ok(());
        }
        if !self
            .person
            .ready("move in small fast circles for 5 seconds")?
        {
            self.report.check("fast motion", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::MOTION);
        let wait = Duration::from_secs(5);
        let log = gather(self.person, &events, "move in small circles now", wait)?;
        let count = log
            .iter()
            .filter(|e| matches!(e, Event::Motion { .. }))
            .count();
        self.report.check(
            "fast motion, events per second and losses",
            Outcome::Noted,
            &format!(
                "about {:.0} events a second, {}",
                count as f64 / self.person.scaled(wait).as_secs_f64(),
                tally(&log)
            ),
        );
        Ok(())
    }
}
