use super::*;
impl Checks<'_> {
    pub(super) fn axis_lock(&mut self, axis: Axis) -> Run<()> {
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
        let text = format!("move your mouse while {along} is locked");
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
        self.person.action(
            if one_way {
                "move your mouse now, only left should work"
            } else {
                "move your mouse in every direction now"
            },
            Duration::from_secs(8),
        );
        let mut seen = Vec::new();
        for step in 0..4 {
            if one_way && step == 2 {
                done = done
                    .and(self.device.unlock_axis(axis, Direction::Positive))
                    .and(self.device.lock_axis(axis, Direction::Negative));
                self.person
                    .cue("move your mouse now, only right should work");
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
}
