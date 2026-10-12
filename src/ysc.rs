use std::time::Duration;

use anybox::{Action, Connect, Device, Event, Family};

use crate::checks::{F13, F14, close, closing, drain, tally};
use crate::person::{Person, Phase, Run};
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
    if !person.ready(
        "we'll check how the box refuses keys without a keyboard, hands off the whole time",
    )? {
        report.check("no keyboard diagnostic", Outcome::Skipped, "");
        return Ok(());
    }
    person.action("hands off, don't touch anything", Duration::from_secs(1));

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
    person.action("hands off, don't touch anything", Duration::from_secs(1));
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
            Outcome::Noted
        };
        report.check(name, outcome, &detail);

        let plugged = if person.keyboard() {
            Some(true)
        } else {
            person.yes_no("replace the mouse with a keyboard in the box, if you have one")?
        };
        if plugged == Some(true) {
            if !person.confirm(if person.keyboard() {
                "plug the keyboard back into the box now"
            } else {
                "unplug the mouse, then plug the keyboard into the box"
            })? {
                return Ok(false);
            }
            report.line("attached device changed to keyboard for this diagnostic");
            if let Some(ui) = &person.ui {
                ui.change(|v| {
                    v.keyboard = true;
                    v.require_area = false;
                    v.phase = Phase::Active;
                });
            }
            person.action("hands off, don't touch anything", Duration::from_secs(2));
            person.wait(Duration::from_secs(2))?;
            report.line(&format!(
                "events during the swap {}",
                tally(&drain(&events))
            ));
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
            person.pad_clear();
            person.action("hands off, don't touch anything", Duration::from_secs(1));
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
            match person.seen() {
                Some(seen) => report.check(
                    "F14 after keyboard return reached the PC",
                    if crate::pad::key_pair(&seen, "F14") {
                        Outcome::Passed
                    } else {
                        Outcome::Failed
                    },
                    &crate::pad::summary(&seen),
                ),
                None => report.check(
                    "F14 after keyboard return reached the PC",
                    Outcome::Noted,
                    "no window in a dry run",
                ),
            }
            Ok(())
        })?,
    }
    Ok(())
}

/// anybox refuses to drive a YSC in debug mode. it needs the vendor tool, so it's optional
pub fn debug_mode(port: &str, person: &Person, report: &mut Report) -> Run<()> {
    let name = "open refused in debug mode";
    // set first, so skipping or closing after turning it on still shows how to turn it off
    if let Some(ui) = &person.ui {
        ui.change(|v| v.restore_debug = true);
    }
    if !person.confirm("turn debug mode on, then close the ysc tool")? {
        report.check(name, Outcome::Skipped, "");
        return Ok(());
    }
    if let Some(ui) = &person.ui {
        ui.change(|v| {
            v.restore_debug = true;
            v.hint = "turn debug mode off, then close the ysc tool before you leave".into();
            v.require_area = false;
        });
    }
    match open(port) {
        Ok(device) => {
            report.check(name, Outcome::Failed, "it opened");
            close(&device, person, report);
        }
        Err(error) => report.check(name, if matches!(&error, anybox::Error::Rejected { action: Action::Connect, detail, .. } if detail == "the YSC box is in debug mode, leave debug mode in the vendor tool and connect again") { Outcome::Passed } else { Outcome::Noted }, &format!("{error}, cmd 702 and absence of setters remain in the trace")),
    }
    if !person.confirm("turn debug mode off, then close the ysc tool")? {
        report.check(
            "debug mode restoration",
            Outcome::Noted,
            "not confirmed, turn debug mode off before you leave",
        );
        return Ok(());
    }
    match open(port) {
        Ok(device) => {
            report.check("normal open after debug mode", Outcome::Passed, "");
            close(&device, person, report);
            if let Some(ui) = &person.ui {
                ui.change(|v| {
                    v.restore_debug = false;
                    v.hint.clear();
                });
            }
        }
        Err(error) => report.check(
            "normal open after debug mode",
            Outcome::Failed,
            &error.to_string(),
        ),
    }
    Ok(())
}
