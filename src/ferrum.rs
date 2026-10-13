use std::io::{Read as _, Write as _};
use std::time::Duration;

use crate::person::{Person, Run};
use crate::report::{Outcome, Report};

fn ask(port: &str, bytes: &[u8]) -> Result<String, String> {
    let mut port = serialport::new(port, 115_200)
        .timeout(Duration::from_millis(300))
        .open()
        .map_err(|error| format!("couldn't open it: {error}"))?;
    port.write_all(bytes)
        .map_err(|error| format!("couldn't write: {error}"))?;
    std::thread::sleep(Duration::from_millis(200));
    let mut chunk = [0u8; 256];
    match port.read(&mut chunk) {
        Ok(n) => Ok(String::from_utf8_lossy(&chunk[..n]).into_owned()),
        Err(error) => Ok(format!("nothing back: {error}")),
    }
}

/// the commands of `a_press_survives_the_port_closing`, on the bare port, since `Device::close` lets go of
/// everything first. nothing in between can stop the run, so the release always goes out
pub fn press_through_close(port: &str, person: &Person, report: &mut Report) -> Run<()> {
    let name = "a press survives the port closing";
    if !person
        .ready("anybox holds left across a port reopen, then lets go. hands off the whole time")?
    {
        report.check(name, Outcome::Skipped, "");
        return Ok(());
    }
    person.pad_clear();
    person.action("hands off, don't touch anything", Duration::from_secs(2));
    let before = ask(port, b"km.left()\r\n");
    let press = ask(port, b"km.left(1)\r\n");
    let after = ask(port, b"km.left()\r\n");
    let mut release = ask(port, b"km.left(0)\r\n");
    if release.is_err() {
        if let Some(ui) = &person.ui {
            ui.change(|v| v.cleanup_failed = true);
        }
        std::thread::sleep(Duration::from_millis(500));
        release = ask(port, b"km.left(0)\r\n");
    }
    std::thread::sleep(Duration::from_millis(300));
    let seen = person.seen();
    let window = seen
        .as_ref()
        .map(|seen| crate::pad::buttons(seen))
        .unwrap_or_default();
    if seen
        .as_ref()
        .is_some_and(|seen| crate::pad::releases(seen, anybox::Button::Left) == 0)
    {
        if let Some(ui) = &person.ui {
            ui.change(|v| v.cleanup_failed = true);
        }
        person.say("left's release wasn't confirmed, if anything stays held, unplug the box");
    }
    let detail = format!(
        "before {before:?}, press {press:?}, after reopening {after:?}, release {release:?}, the window saw {window}"
    );
    if release.is_err() {
        report.check(name, Outcome::Failed, &detail);
        person.say(
            "couldn't send the release for left, so if left is stuck down, unplug the Ferrum and plug it back in",
        );
    } else {
        report.check(name, Outcome::Noted, &detail);
    }
    person.stop();
    Ok(())
}

/// the app's Serial API tab has presets that make its port look like another chip. what Windows then lists for the
/// port tells us whether anybox could ever mistake it for a CH343 box or the box's own CP210x port
pub fn presets(person: &Person, report: &mut Report) -> Run<()> {
    let name = "port list with each preset";
    if !person.ready("last thing, you click each port preset in the ferrum app's Serial API tab and we list the ports")? {
        report.check(name, Outcome::Skipped, "");
        return Ok(());
    }
    for preset in ["CH340", "CH343", "CH347", "CP2102"] {
        if person.confirm(&format!(
            "click the {preset} preset in the ferrum app's Serial API tab"
        ))? {
            person.wait(Duration::from_secs(2))?;
            crate::setup::ports(person, report, &format!("with the {preset} preset"));
        } else {
            report.check(name, Outcome::Skipped, &format!("{preset} not clicked"));
        }
    }
    report.check(name, Outcome::Noted, "the port lists are above");
    person.cue("put the preset back to the one you had before");
    Ok(())
}
