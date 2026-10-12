use std::io::{Read as _, Write as _};
use std::time::Duration;

use crate::person::{Person, Run};
use crate::report::{Outcome, Report};

/// one command on the bare port, opened fresh each time the way an app that crashed and restarted would. what
/// came back, or why nothing did
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
    if !person.ready(
        "next it presses left on the bare port, closes and reopens it and reads left back, then lets go. put your \
         cursor over an empty spot of the desktop",
    )? {
        report.check(name, Outcome::Skipped, "");
        return Ok(());
    }
    let before = ask(port, b"km.left()\r\n");
    let press = ask(port, b"km.left(1)\r\n");
    let after = ask(port, b"km.left()\r\n");
    let mut release = ask(port, b"km.left(0)\r\n");
    if release.is_err() {
        std::thread::sleep(Duration::from_millis(500));
        release = ask(port, b"km.left(0)\r\n");
    }
    let detail = format!(
        "before {before:?}, press {press:?}, after reopening {after:?}, release {release:?}"
    );
    if release.is_err() {
        report.check(name, Outcome::Failed, &detail);
        person.say(
            "couldn't send the release for left, so if left is stuck down, unplug the Ferrum and plug it back in",
        );
    } else {
        report.check(name, Outcome::Noted, &detail);
    }
    Ok(())
}
