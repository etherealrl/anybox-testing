use super::*;
impl Checks<'_> {
    pub fn automatic(&mut self) -> Run<()> {
        if !self
            .person
            .ready("anybox sends small moves, clicks, scrolls and keys. hands off the whole time")?
        {
            self.report.check("automatic output", Outcome::Skipped, "");
            return Ok(());
        }
        let before = self.report.failed();
        if !self.person.can_position() {
            for name in [
                "small moves",
                "wheel output",
                "software buttons",
                "timed clicks",
                "repeated presses",
            ] {
                self.report.check(
                    name,
                    Outcome::NotAvailable,
                    "no pc pointing device for positioning",
                );
            }
            self.refusals();
            self.keys()?;
            self.person.stop();
            return Ok(());
        }
        let events = self.device.events();
        let short = Duration::from_millis(500);
        let mut moves = Vec::new();
        if supported(self.device, Action::Move) {
            for (dx, dy) in [(100, 0), (0, 100), (-100, 0), (0, -100)] {
                self.person.pad_clear();
                self.person.action("hands off, don't touch anything", short);
                let at = Instant::now();
                let call = self.device.move_by(dx, dy);
                self.report
                    .line(&format!("move {dx} {dy} call time {:?}", at.elapsed()));
                let mut log = Vec::new();
                collect(self.person, &events, short, &mut log)?;
                moves.push((dx, dy, call, self.person.seen()));
            }
            let raw = moves.iter().any(|(_, _, _, seen)| {
                seen.as_ref()
                    .is_some_and(|w| w.iter().any(|s| matches!(s.input, pad::Input::Moved(..))))
            });
            for (dx, dy, call, seen) in moves {
                let name = format!("move {dx} {dy} reached the PC");
                match (call, seen) {
                    (Err(error), _) => {
                        self.report
                            .check(&name, Outcome::Failed, &error.to_string())
                    }
                    (Ok(()), None) => {
                        self.report
                            .check(&name, Outcome::Noted, "no window in a dry run")
                    }
                    (Ok(()), Some(_)) if !raw => {
                        self.report
                            .check(&name, Outcome::Noted, "no raw motion from windows")
                    }
                    (Ok(()), Some(w)) => {
                        self.report
                            .check(&name, passed_if(went(&w, dx, dy)), &pad::summary(&w))
                    }
                }
            }
            if self.report.failed() > before {
                return Ok(());
            }
        } else {
            self.report
                .check("small moves", Outcome::NotAvailable, "no move output");
        }
        for name in ["zero move", "wheel 0"] {
            if (name == "wheel 0" && !supported(self.device, Action::Wheel))
                || (name == "zero move" && !supported(self.device, Action::Move))
            {
                self.report
                    .check(name, Outcome::NotAvailable, "no wheel output");
                continue;
            }
            self.person.pad_clear();
            let call = if name == "zero move" {
                self.device.move_by(0, 0)
            } else {
                self.device.wheel(0)
            };
            self.person.wait(short)?;
            self.window(name, call, |w| {
                pad::moved(w) == (0.0, 0.0) && pad::wheel(w) == 0.0
            });
        }
        if supported(self.device, Action::Wheel) {
            for notches in [1i16, -1] {
                self.person.pad_clear();
                self.person.action("hands off, don't touch anything", short);
                let call = self.device.wheel(notches);
                self.person.wait(short)?;
                self.window(&format!("wheel {notches} reached the PC"), call, |w| {
                    pad::wheel(w) != 0.0 && pad::wheel(w).signum() == f32::from(notches).signum()
                });
                if self.report.failed() > before {
                    return Ok(());
                }
            }
            if quirks(self.device, Action::Wheel).contains(&Quirk::NotchesPerCommand(1)) {
                let call = self.device.wheel(2);
                self.report.check(
                    "wheel 2 refused before sending",
                    passed_if(matches!(
                        call,
                        Err(Error::Unsupported { .. } | Error::InvalidArgument { .. })
                    )),
                    &format!("{call:?}, see trace for wire evidence"),
                );
            }
        } else {
            self.report
                .check("wheel", Outcome::NotAvailable, "no wheel output");
        }
        for button in BUTTONS {
            if !supported(self.device, Action::Press) || !supported(self.device, Action::Release) {
                self.report.check(
                    &format!("{button:?} output"),
                    Outcome::NotAvailable,
                    "no button press and release",
                );
                continue;
            }
            self.person.pad_clear();
            self.person.action("hands off, don't touch anything", short);
            let pressed = self.device.press(button);
            let waited = self.person.wait(Duration::from_millis(60));
            let released = self.device.release(button);
            waited?;
            self.person.wait(short)?;
            self.window(
                &format!("press and release {button:?} reached the PC"),
                pressed.and(released),
                |w| pad::presses(w, button) > 0 && pad::ends_down(w, button) == Some(false),
            );
            if self.report.failed() > before {
                return Ok(());
            }
        }
        self.person.pad_clear();
        let first = self.device.press(Button::Middle);
        self.person.wait(Duration::from_millis(100))?;
        let second = self.device.press(Button::Middle);
        self.person.wait(Duration::from_millis(100))?;
        let released = self.device.release(Button::Middle);
        self.person.wait(short)?;
        self.window(
            "repeated middle presses finish released",
            first.and(second).and(released),
            |w| pad::ends_down(w, Button::Middle) == Some(false),
        );
        if self.report.failed() > before {
            return Ok(());
        }
        if supported(self.device, Action::Click) {
            self.person.pad_clear();
            let call = self
                .device
                .click_with(Button::Right, Click::hold(Duration::from_millis(80)));
            self.person.wait(short)?;
            if let Some(w) = self.person.seen() {
                self.report.line(&format!(
                    "requested hold 80 ms, observed {:?}",
                    pad::first_hold(&w, Button::Right)
                ));
            }
            self.window("right click with an 80 ms hold", call, |w| {
                pad::first_hold(w, Button::Right).is_some()
            });
            for number in 1..=5 {
                self.person.pad_clear();
                let call = self.device.click(Button::Middle);
                self.person.wait(short)?;
                self.window(&format!("middle repeat click {number}"), call, |w| {
                    pad::presses(w, Button::Middle) == 1 && pad::releases(w, Button::Middle) == 1
                });
                if self.report.failed() > before {
                    return Ok(());
                }
            }
        } else {
            self.report.check(
                "timed and repeated clicks",
                Outcome::NotAvailable,
                "no click output",
            );
        }
        self.refusals();
        self.keys()?;
        self.report.line(&format!(
            "output event observations {}",
            tally(&drain(&events))
        ));
        self.person.stop();
        Ok(())
    }

    pub fn boundary(&mut self) -> Run<()> {
        if !self.person.can_position() {
            self.report.check(
                "boundary moves",
                Outcome::NotAvailable,
                "no pc pointing device for positioning",
            );
            return Ok(());
        }
        if !self
            .person
            .ready("anybox sends huge moves, your cursor will hit the screen edge. hands off until it's done")?
        {
            self.report
                .check("i16 boundary moves", Outcome::Skipped, "");
            return Ok(());
        }
        if let Some(ui) = &self.person.ui {
            ui.change(|v| v.require_area = false);
        }
        let ran = self.boundary_moves();
        if let Some(ui) = &self.person.ui {
            ui.change(|v| v.require_area = v.can_position);
        }
        ran?;
        self.person.stop();
        self.person.cue("bring your cursor back to the middle");
        Ok(())
    }

    fn boundary_moves(&mut self) -> Run<()> {
        for axis in [Axis::X, Axis::Y] {
            for amount in [i16::MAX, -i16::MAX, i16::MIN, i16::MAX, 1] {
                self.person.pad_clear();
                self.person.action(
                    "hands off, don't touch anything",
                    Duration::from_millis(500),
                );
                let (dx, dy) = if axis == Axis::X {
                    (amount, 0)
                } else {
                    (0, amount)
                };
                let at = Instant::now();
                let call = self.device.move_by(dx, dy);
                let duration = at.elapsed();
                self.person.wait(Duration::from_millis(500))?;
                self.report.check(
                    &format!("boundary {dx} {dy}"),
                    if call.is_ok() {
                        Outcome::Noted
                    } else {
                        Outcome::Failed
                    },
                    &format!(
                        "call {call:?}, call time {duration:?}, delivery unverified, {}",
                        self.person
                            .seen()
                            .map(|w| pad::summary(&w))
                            .unwrap_or_else(|| "no window in a dry run".into())
                    ),
                );
            }
        }
        Ok(())
    }
    fn refusals(&mut self) {
        self.report.line("refusal checks judge API errors, inspect trace for absence of setters; untraced transports cannot prove no wire write");
        if quirks(self.device, Action::LockAxis).contains(&Quirk::WholeAxisOnly) {
            match self.device.lock_axis(Axis::X, Direction::Positive) {
                Err(Error::Unsupported { .. }) => {
                    self.report
                        .check("one way X lock refused", Outcome::Passed, "")
                }
                other => {
                    result(
                        self.report,
                        "unlock unexpected one-way lock",
                        self.device.unlock_axis(Axis::X, Direction::Both),
                        "",
                    );
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
                    if let Err(error) = &undone {
                        self.report
                            .check("axis cleanup", Outcome::Failed, &error.to_string());
                    }
                    self.report.check(
                        name,
                        Outcome::Noted,
                        &format!("the box took it, unlocking it again: {undone:?}"),
                    )
                }
                Err(error) => self.report.check(
                    name,
                    if matches!(
                        error,
                        Error::Unsupported { .. } | Error::InvalidArgument { .. }
                    ) {
                        Outcome::Passed
                    } else {
                        Outcome::Failed
                    },
                    &error.to_string(),
                ),
            }
        }
        if !supported(self.device, Action::LockKey) {
            match self.device.lock_key(F13) {
                Err(Error::Unsupported { .. }) => {
                    self.report.check("key lock refused", Outcome::Passed, "")
                }
                other => {
                    result(
                        self.report,
                        "unlock unexpected key lock",
                        self.device.unlock_key(F13),
                        "",
                    );
                    self.report
                        .check("key lock refused", Outcome::Failed, &format!("{other:?}"))
                }
            }
        }
    }

    fn keys(&mut self) -> Run<()> {
        let events = self.device.events();
        if !supported(self.device, Action::KeyPress) || !supported(self.device, Action::KeyRelease)
        {
            self.report.check(
                "F13 press and release",
                Outcome::NotAvailable,
                "no key press and release",
            );
        } else {
            self.person.pad_clear();
            self.person
                .action("hands off, don't touch anything", Duration::from_secs(1));
            let pressed = self.device.press_key(F13);
            let waited = self.person.wait(Duration::from_millis(100));
            let released = self.device.release_key(F13);
            waited?;
            let mut log = Vec::new();
            collect(self.person, &events, Duration::from_secs(1), &mut log)?;
            if log.iter().any(|e| matches!(e, Event::Rejected { .. })) {
                self.report.check(
                    "key output refused asynchronously",
                    Outcome::Noted,
                    &format!("press {pressed:?}, release {released:?}, {}", tally(&log)),
                );
                self.identity();
                return Ok(());
            }
            self.window(
                "F13 press and release reached the PC",
                pressed.and(released),
                |w| pad::key_pair(w, "F13"),
            );
        }
        if supported(self.device, Action::KeyTap) {
            self.person.pad_clear();
            let tapped = self.device.tap_key(F14);
            self.person.wait(Duration::from_millis(500))?;
            self.window("F14 tap reached the PC", tapped, |w| {
                pad::key_pair(w, "F14")
            });
        } else {
            self.report
                .check("F14 tap", Outcome::NotAvailable, "no key tap");
        }
        if supported(self.device, Action::KeyPress)
            && supported(self.device, Action::KeyRelease)
            && supported(self.device, Action::KeyTap)
        {
            self.person.pad_clear();
            let held = self.device.press_key(Key::LEFT_SHIFT);
            self.person.wait(Duration::from_millis(100))?;
            let tapped = self.device.tap_key(F13);
            self.person.wait(Duration::from_millis(100))?;
            let released = self.device.release_key(Key::LEFT_SHIFT);
            self.person.wait(Duration::from_millis(500))?;
            self.window(
                "F13 tap with left shift held",
                held.and(tapped).and(released),
                |w| {
                    pad::key_pair(w, "F13")
                        && w.iter().any(|s| matches!(s.input, pad::Input::Shift(true)))
                        && matches!(
                            w.iter().rev().find_map(
                                |s| if let pad::Input::Shift(shift) = s.input {
                                    Some(shift)
                                } else {
                                    None
                                }
                            ),
                            Some(false)
                        )
                },
            );
        } else {
            self.report.check(
                "shift with F13",
                Outcome::NotAvailable,
                "no key press, release, and tap",
            );
        }
        let mut log = Vec::new();
        collect(self.person, &events, Duration::from_secs(1), &mut log)?;
        self.report.check(
            "no key refused afterwards",
            passed_if(!log.iter().any(|e| matches!(e, Event::Rejected { .. }))),
            &tally(&log),
        );
        Ok(())
    }
}
