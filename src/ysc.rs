use std::time::Duration;

use anybox::{Action, Connect, Device, Event, Family};

use crate::checks::{F13, F14, close, closing, drain, tally};
use crate::person::{Person, Run};
use crate::report::{Outcome, Report};

pub fn open(port: &str) -> anybox::Result<Device> {
    Connect::port(port).family(Family::Ysc).open()
}

fn keys(device: &Device) -> bool {
    let capabilities = &device.snapshot().capabilities;
    capabilities.get(Action::KeyPress).is_supported()
        || capabilities.get(Action::KeyTap).is_supported()
}

fn refused_presses(events: &[Event]) -> u64 {
    events
        .iter()
        .map(|e| match e {
            Event::Rejected {
                action: Action::KeyPress,
                count,
                ..
            } => *count,
            _ => 0,
        })
        .sum()
}

/// with no keyboard behind it the box answers a key with a 404. anybox only blames a refusal on a press when
/// presses were the only key action sent, so each case needs its own fresh open
pub fn no_keyboard(port: &str, person: &Person, report: &mut Report) -> Run<()> {
    person.say(
        "next a few quick opens of the YSC to see what it says to keys without a keyboard, nothing to do",
    );

    let name = "no keyboard, presses only";
    match open(port) {
        Err(error) => report.check(name, Outcome::Failed, &error.to_string()),
        Ok(first) => closing(&first, person, report, |report| {
            let events = first.events();
            let pressed = first.press_key(F13);
            person.wait(Duration::from_secs(1))?;
            let seen = drain(&events);
            let refused = refused_presses(&seen);
            let ok = refused > 0 && !keys(&first);
            report.check(
                name,
                if ok { Outcome::Passed } else { Outcome::Failed },
                &format!(
                    "press {pressed:?}, keys still there {}, events: {}",
                    keys(&first),
                    tally(&seen)
                ),
            );
            Ok(())
        })?,
    }

    let name = "no keyboard, a press and a release";
    let second = match open(port) {
        Ok(device) => device,
        Err(error) => {
            report.check(name, Outcome::Failed, &error.to_string());
            return Ok(());
        }
    };
    let plugged = closing(&second, person, report, |report| {
        let events = second.events();
        let sent = second.press_key(F13).and(second.release_key(F13));
        person.wait(Duration::from_secs(1))?;
        let seen = drain(&events);
        let refused = refused_presses(&seen);
        let detail = format!(
            "sent {sent:?}, keys still there {}, events: {}",
            keys(&second),
            tally(&seen)
        );
        let outcome = if keys(&second) {
            Outcome::Failed
        } else if refused > 0 {
            // the 404 can come back before the release went out, then blaming the press is right. the trace
            // shows which way it went
            Outcome::Noted
        } else {
            Outcome::Passed
        };
        report.check(name, outcome, &detail);

        let plugged = person.yes_no(
            "if you have a keyboard, plug it into the YSC now and answer y once it's in, otherwise n",
        )?;
        if plugged == Some(true) {
            person.wait(Duration::from_secs(2))?;
            report.check(
                "keys stay off on the open that saw the 404",
                if keys(&second) {
                    Outcome::Failed
                } else {
                    Outcome::Passed
                },
                "",
            );
        }
        Ok(plugged == Some(true))
    })?;
    drop(second);
    if !plugged {
        report.check(
            "keys back after a reopen",
            Outcome::NotAvailable,
            "no keyboard to plug in",
        );
        return Ok(());
    }

    let name = "keys back after a reopen";
    match open(port) {
        Err(error) => report.check(name, Outcome::Failed, &error.to_string()),
        Ok(third) => closing(&third, person, report, |report| {
            let events = third.events();
            let tapped = third.tap_key(F14);
            person.wait(Duration::from_secs(1))?;
            let seen = drain(&events);
            let refused = seen.iter().any(|e| matches!(e, Event::Rejected { .. }));
            let ok = keys(&third) && tapped.is_ok() && !refused;
            report.check(
                name,
                if ok { Outcome::Passed } else { Outcome::Failed },
                &format!("F14 tap {tapped:?}, events: {}", tally(&seen)),
            );
            Ok(())
        })?,
    }
    Ok(())
}

/// anybox refuses to drive a YSC in debug mode. it needs the vendor tool, so it's optional
pub fn debug_mode(port: &str, person: &Person, report: &mut Report) -> Run<()> {
    let name = "open refused in debug mode";
    let want = person.yes_no(
        "optional: want to check what happens in debug mode? you need the YSC tool to turn it on and off again",
    )?;
    if want != Some(true) {
        report.check(name, Outcome::Skipped, "");
        return Ok(());
    }
    if !person
        .ready("turn debug mode on in the YSC tool, then CLOSE the tool so the port is free")?
    {
        report.check(name, Outcome::Skipped, "");
        return Ok(());
    }
    match open(port) {
        Ok(device) => {
            report.check(name, Outcome::Failed, "it opened");
            close(&device, person, report);
        }
        Err(error) => report.check(name, Outcome::Noted, &error.to_string()),
    }
    person.ready("now turn debug mode off again in the YSC tool and close the tool")?;
    Ok(())
}
