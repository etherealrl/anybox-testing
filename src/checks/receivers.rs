use super::*;

impl Checks<'_> {
    pub(super) fn receivers(&mut self) -> Run<()> {
        let name = "a buttons only receiver next to an everything receiver";
        let keyboard = self.person.keyboard();
        let action = if keyboard {
            Action::KeyEvents
        } else {
            Action::ButtonEvents
        };
        if !supported(self.device, action) {
            self.report.check(
                name,
                Outcome::NotAvailable,
                "no stream for the attached device",
            );
            return Ok(());
        }
        if !self.person.ready(if keyboard {
            "type a few letters while we check two event streams"
        } else {
            "click and move while we check two event streams"
        })? {
            self.report.check(name, Outcome::Skipped, "");
            return Ok(());
        }
        let buttons = self.device.events_of(EventKinds::BUTTONS);
        let all = self.device.events();
        drain(&buttons);
        drain(&all);
        self.person.action(
            if keyboard {
                "type \"abc\" now"
            } else {
                "click and move your mouse now"
            },
            Duration::from_secs(5),
        );
        let mut only = Vec::new();
        let mut everything = Vec::new();
        let until = Instant::now() + self.person.scaled(Duration::from_secs(5));
        while Instant::now() < until {
            self.person.stopped()?;
            collect(
                self.person,
                &all,
                Duration::from_millis(50),
                &mut everything,
            )?;
            only.extend(drain(&buttons));
        }
        only.extend(drain(&buttons));
        everything.extend(drain(&all));
        let filtered = only.iter().all(|e| {
            !matches!(
                e,
                Event::Motion { .. } | Event::Key { .. } | Event::Media { .. }
            )
        });
        let delivered = if keyboard {
            everything.iter().any(|e| matches!(e, Event::Key { .. }))
        } else {
            buttons_in(&only) > 0
                && buttons_in(&everything) > 0
                && (!supported(self.device, Action::MotionEvents)
                    || everything.iter().any(|e| matches!(e, Event::Motion { .. })))
        };
        self.report.check(
            name,
            passed_if(filtered && delivered),
            &format!("buttons {}, all {}", tally(&only), tally(&everything)),
        );
        if keyboard && self.person.can_position() {
            let call = self.device.click(Button::Middle);
            self.report.check(
                "injected middle receiver observation",
                Outcome::Noted,
                &format!("{call:?}, {}", tally(&drain(&all))),
            );
            self.report.check(
                "physical button filtering",
                Outcome::NotAvailable,
                "a keyboard is attached",
            );
        }
        drop((all, buttons));
        self.person
            .action("stop now, hands off for a moment", Duration::from_secs(2));
        self.person.wait(Duration::from_secs(2))?;
        let again = self.device.events();
        let log = self
            .acquire(
                &again,
                if keyboard {
                    "press \"a\" once now"
                } else {
                    "click left once now"
                },
                |log, _| {
                    if keyboard {
                        [true, false].into_iter().all(|down| log.iter().any(|e| matches!(e, Event::Key { key: Key(4), pressed, .. } if *pressed == down)))
                    } else {
                        [true, false].into_iter().all(|down| log.iter().any(|e| matches!(e, Event::Button { button: Button::Left, pressed, .. } if *pressed == down)))
                    }
                },
            )?
            .unwrap_or_default();
        self.report.check("last receiver dropped, then a new one", passed_if(if keyboard {
            [true, false].into_iter().all(|down| log.iter().any(|e| matches!(e, Event::Key { key: Key(4), pressed, .. } if *pressed == down)))
        } else { [true, false].into_iter().all(|down| log.iter().any(|e| matches!(e, Event::Button { button: Button::Left, pressed, .. } if *pressed == down))) }), &format!("{}, trace records off and on where available", tally(&log)));
        self.person.stop();
        Ok(())
    }
}
