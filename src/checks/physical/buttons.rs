use super::*;

impl Checks<'_> {
    pub(super) fn button_locks(&mut self) -> Run<()> {
        if !supported(self.device, Action::LockButton) {
            self.report
                .check("button locks", Outcome::NotAvailable, "no button locks");
            return Ok(());
        }
        if !self.person.ready(
            "click each button twice while it's locked, then hands off while anybox clicks it",
        )? {
            self.report.check("button locks", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events_of(EventKinds::BUTTONS);
        for button in BUTTONS {
            if self.person.missing(button) {
                self.report.check(
                    &format!("{} lock", button_name(button)),
                    Outcome::NotAvailable,
                    "this mouse doesn't have it",
                );
                continue;
            }
            if self
                .person
                .ui
                .as_ref()
                .is_some_and(|ui| !ui.view.lock().unwrap().physical_buttons.contains(&button))
            {
                self.report.check(
                    &format!("{} lock", button_name(button)),
                    Outcome::NotAvailable,
                    "physical delivery wasn't established",
                );
                continue;
            }
            let locked = self.device.lock_button(button);
            self.person.pad_clear();
            drain(&events);
            let log = gather(
                self.person,
                &events,
                &format!("click the {} twice now", button_name(button)),
                Duration::from_secs(5),
            );
            let log = match log {
                Ok(log) => log,
                Err(error) => {
                    result(
                        self.report,
                        "unlock on stop",
                        self.device.unlock_button(button),
                        "",
                    );
                    return Err(error);
                }
            };
            let physical = presses_of(&log, button) > 0;
            let suppressed = self.person.seen().map(|w| pad::presses(&w, button) == 0);
            // a moment to take your hands off before anybox clicks, the bar above shows it
            self.person.action(
                "stop now, hands off, anybox clicks it next",
                Duration::from_secs(2),
            );
            self.person.wait(Duration::from_secs(2))?;
            self.person.pad_clear();
            let clicked = self.device.click(button);
            let waited = self.person.wait(Duration::from_millis(500));
            let unlocked = self.device.unlock_button(button);
            waited?;
            self.report.check(
                &format!("{button:?} locked"),
                if locked.is_err() || unlocked.is_err() {
                    Outcome::Failed
                } else if suppressed.is_none() || !physical {
                    Outcome::Noted
                } else {
                    passed_if(suppressed == Some(true))
                },
                &format!("lock {locked:?}, unlock {unlocked:?}, {}", tally(&log)),
            );
            self.window(
                &format!(
                    "anybox's {} click goes through its lock",
                    button_name(button)
                ),
                clicked,
                |w| window_pair(w, button),
            );
        }
        self.person.stop();
        Ok(())
    }
}
