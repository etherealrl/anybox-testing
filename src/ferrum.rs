use std::io::{Read as _, Write as _};
use std::time::Duration;

use crate::person::{Person, Run};
use crate::report::{Outcome, Report};

/// the commands of `a_press_survives_the_port_closing`, on the bare port, since `Device::close` lets go of
/// everything first. each one opens the port fresh, the way an app that crashed and restarted would
fn ask(port: &str, bytes: &[u8]) -> String {
    let opened = serialport::new(port, 115_200)
        .timeout(Duration::from_millis(300))
        .open();
    let mut port = match opened {
        Ok(port) => port,
        Err(error) => return format!("couldn't open it: {error}"),
    };
    if let Err(error) = port.write_all(bytes) {
        return format!("couldn't write: {error}");
    }
    std::thread::sleep(Duration::from_millis(200));
    let mut chunk = [0u8; 256];
    match port.read(&mut chunk) {
        Ok(n) => String::from_utf8_lossy(&chunk[..n]).into_owned(),
        Err(error) => format!("nothing back: {error}"),
    }
}

/// sends the release on the way out, even when a step in between failed or the run got stopped
struct LetGo<'a>(&'a str);

impl Drop for LetGo<'_> {
    fn drop(&mut self) {
        let _ = ask(self.0, b"km.left(0)\r\n");
    }
}

pub fn press_through_close(port: &str, person: &Person, report: &mut Report) -> Run<()> {
    let name = "a press survives the port closing";
    if !person.ready(
        "next it presses left on the bare port, closes and reopens it and reads left back, then lets go. put your \
         cursor over an empty spot of the desktop",
    )? {
        report.check(name, Outcome::Skipped, "");
        return Ok(());
    }
    let _let_go = LetGo(port);
    let before = ask(port, b"km.left()\r\n");
    let press = ask(port, b"km.left(1)\r\n");
    let after = ask(port, b"km.left()\r\n");
    let release = ask(port, b"km.left(0)\r\n");
    report.check(
        name,
        Outcome::Noted,
        &format!(
            "before {before:?}, press {press:?}, after reopening {after:?}, release {release:?}"
        ),
    );
    Ok(())
}
