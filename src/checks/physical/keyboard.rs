use super::*;

impl Checks<'_> {
    pub(super) fn physical_keys(&mut self) -> Run<()> {
        if !self
            .person
            .ready("press and release \"a\", \"b\", then \"c\" on the keyboard in the box")?
        {
            self.report.check("physical keys", Outcome::Skipped, "");
            return Ok(());
        }
        let events = self.device.events();
        for (letter, usage) in [("a", 4), ("b", 5), ("c", 6)] {
            let key = Key(usage);
            let log = self
                .acquire(
                    &events,
                    &format!("press \"{letter}\" once now"),
                    |log, window| {
                        pad::key_pair(window, &letter.to_uppercase())
                            && (!supported(self.device, Action::KeyEvents) || key_pair(log, key))
                    },
                )?
                .unwrap_or_default();
            self.report.check(
                &format!("physical {letter} events"),
                if supported(self.device, Action::KeyEvents) {
                    passed_if(key_pair(&log, key))
                } else {
                    Outcome::NotAvailable
                },
                &tally(&log),
            );
            self.window(&format!("physical {letter} reached the PC"), Ok(()), |w| {
                pad::key_pair(w, &letter.to_uppercase())
            });
        }
        self.person.stop();
        Ok(())
    }
    pub(super) fn key_merge(&mut self) -> Run<()> {
        if !supported(self.device, Action::KeyPress) || !supported(self.device, Action::KeyRelease)
        {
            self.report.check(
                "key merging",
                Outcome::NotAvailable,
                "no key press and release",
            );
            return Ok(());
        }
        if !self
            .person
            .ready("with caps lock off, type \"abc\" without holding shift")?
        {
            self.report.check("key merging", Outcome::Skipped, "");
            return Ok(());
        }
        self.person.pad_clear();
        self.person
            .action("type \"abc\" now", Duration::from_secs(10));
        self.person.wait(Duration::from_secs(10))?;
        let baseline = self.person.seen().unwrap_or_default();
        if pad::text(&baseline) != "abc"
            || baseline
                .iter()
                .any(|s| matches!(s.input, pad::Input::Paste))
        {
            self.report.check(
                "shift baseline",
                Outcome::Noted,
                "we need lowercase abc first, turn caps lock off and try again",
            );
            return Ok(());
        }
        self.person.pad_clear();
        let held = self.device.press_key(Key::LEFT_SHIFT);
        self.person.action(
            "type \"abc\" now, don't hold shift yourself",
            Duration::from_secs(6),
        );
        let waited = self.person.wait(Duration::from_secs(6));
        let released = self.device.release_key(Key::LEFT_SHIFT);
        let shifted = self.person.seen().unwrap_or_default();
        waited?;
        self.person.pad_clear();
        self.person
            .action("type \"a\" once more", Duration::from_secs(5));
        self.person.wait(Duration::from_secs(5))?;
        let after = self.person.seen().unwrap_or_default();
        let shift_seen = shifted
            .iter()
            .any(|s| matches!(s.input, pad::Input::Shift(true)));
        let shift_up = after
            .iter()
            .any(|s| matches!(s.input, pad::Input::Shift(false)));
        let ok = held.is_ok()
            && released.is_ok()
            && shift_seen
            && shift_up
            && pad::text(&shifted) == "ABC"
            && pad::text(&after) == "a"
            && !shifted
                .iter()
                .chain(&after)
                .any(|s| matches!(s.input, pad::Input::Paste));
        self.report.check("a key held by anybox merges with your typing", passed_if(ok), &format!("uppercase abc {}, lowercase a {}, shift active {shift_seen}, shift released {shift_up}", pad::text(&shifted) == "ABC", pad::text(&after) == "a"));
        self.person.stop();
        Ok(())
    }
}
