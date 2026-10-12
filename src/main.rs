mod checks;
mod ferrum;
mod person;
mod report;
mod ysc;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anybox::{ClickTiming, Connect, Device, Family, FerrumApi};
use serialport::SerialPortType;

use checks::{Checks, close};
use person::{Person, Run, STOP};
use report::{Outcome, Report};

const YSC_LEFT: &str = "TOWMCULEFT";

fn main() {
    let mut args = std::env::args().skip(1);
    let mut scripted = false;
    let mut out = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dry-run" => scripted = true,
            "--out" => out = PathBuf::from(args.next().expect("--out needs a folder")),
            other => {
                eprintln!("unknown option {other}, the only ones are --dry-run and --out <folder>");
                std::process::exit(2);
            }
        }
    }
    let _ = ctrlc::set_handler(|| STOP.store(true, Ordering::Release));

    let stamp = report::now() as u64;
    let trace = out.join(format!("anybox-trace-{stamp}.tmp"));
    // SAFETY: nothing else runs yet, so no other thread reads the environment while it changes
    unsafe { std::env::set_var("ANYBOX_TRACE", &trace) };

    let person = Person { scripted };
    let mut report = Report::default();
    let ran = if scripted {
        dry_run(&person, &mut report)
    } else {
        run(&person, &mut report)
    };
    if ran.is_err() {
        report.line("\nthe run was stopped before the end");
    }

    // every device and probe is closed by now, so the trace is flushed
    let traced = std::fs::read_to_string(&trace).unwrap_or_default();
    let _ = std::fs::remove_file(&trace);
    let header = format!(
        "anybox-testing {}, anybox {}, {}",
        env!("CARGO_PKG_VERSION"),
        env!("ANYBOX_VERSION"),
        windows_version()
    );
    let name = if scripted {
        "anybox-dry-run"
    } else {
        "anybox-report"
    };
    let file = out.join(format!("{name}-{stamp}.txt"));
    match std::fs::write(&file, report.render(&header, &traced)) {
        Ok(()) => person.say(&format!(
            "\nall done, the report is {}. it's plain text so you can read it before you send it",
            file.display()
        )),
        Err(error) => person.say(&format!(
            "\ncouldn't write the report to {}: {error}",
            file.display()
        )),
    }
    if !scripted {
        person.say("press Enter to close this window");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    if report.failed() > 0 && scripted {
        std::process::exit(1);
    }
}

fn windows_version() -> String {
    std::process::Command::new("cmd")
        .args(["/c", "ver"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|_| std::env::consts::OS.to_string())
}

/// every serial port with what USB says about it, the serial included, because that's how a YSC tells its ports
/// apart
fn ports(report: &mut Report, when: &str) -> Vec<serialport::SerialPortInfo> {
    report.section(&format!("ports {when}"));
    let ports = serialport::available_ports().unwrap_or_default();
    if ports.is_empty() {
        report.line("none");
    }
    for port in &ports {
        let line = match &port.port_type {
            SerialPortType::UsbPort(usb) => format!(
                "{} {:04X}:{:04X} serial {:?}, {:?}, {:?}",
                port.port_name, usb.vid, usb.pid, usb.serial_number, usb.manufacturer, usb.product
            ),
            other => format!("{} {other:?}", port.port_name),
        };
        report.line(&line);
    }
    ports
}

fn is_ysc_left(port: &serialport::SerialPortInfo) -> bool {
    matches!(&port.port_type, SerialPortType::UsbPort(usb)
        if (usb.vid, usb.pid) == (0x1A86, 0xFE0C) && usb.serial_number.as_deref() == Some(YSC_LEFT))
}

/// the questions about what's plugged in, since anybox can't read any of it
fn setup(person: &Person, report: &mut Report, extra: &[&str]) -> Run<()> {
    let mut questions = vec![
        "which mouse is plugged into the box, and at what polling rate?",
        "which keyboard is plugged into the box (or none)?",
    ];
    questions.extend_from_slice(extra);
    for question in questions {
        let answer = person.answer(question, "dry run")?;
        report.line(&format!("{question} {answer}"));
    }
    Ok(())
}

fn common(
    device: &Device,
    person: &Person,
    report: &mut Report,
    what: &str,
    unplug: Option<&dyn Fn()>,
) -> Run<()> {
    let mut checks = Checks {
        device,
        person,
        report,
    };
    checks.identity();
    checks.automatic()?;
    checks.with_them()?;
    checks.reconnect(what, unplug)
}

/// runs the checks and always closes, so a stopped run still lets go of everything
fn on_box(
    device: Device,
    person: &Person,
    report: &mut Report,
    what: &str,
    unplug: Option<&dyn Fn()>,
) -> Run<()> {
    let ran = common(&device, person, report, what, unplug);
    close(&device, person, report);
    ran
}

fn run(person: &Person, report: &mut Report) -> Run<()> {
    person.say(
        "this checks how anybox does with your boxes and writes everything into one report you send back. it moves \
         your cursor a little, clicks right, middle and the side buttons, presses and lets go of left while you're \
         holding it, and holds F13, F14 and shift, it never types anything. close games and anything that reacts \
         to clicks first. if anything ever stays held, unplugging the box lets go of it, and Ctrl+C stops the run \
         at any time\n",
    );
    let found = ports(report, "at the start");

    for port in found.iter().filter(|p| is_ysc_left(p)) {
        let path = port.port_name.as_str();
        report.section(&format!("YSC 8K on {path}"));
        person.say(&format!("\nYSC 8K on {path}"));
        setup(
            person,
            report,
            &[
                "which port on the YSC is the cable to this PC in (what's it labeled)?",
                "is interpolation or a curve turned on in the YSC tool?",
            ],
        )?;
        ysc::debug_mode(path, person, report)?;
        match ysc::open(path) {
            Ok(device) => {
                let keyboard = device
                    .snapshot()
                    .capabilities
                    .get(anybox::Action::KeyPress)
                    .is_supported();
                on_box(device, person, report, "the YSC's cable to this PC", None)?;
                let none = person
                    .yes_no("is the YSC running WITHOUT a keyboard plugged into it right now?")?;
                if none == Some(true) || !keyboard {
                    ysc::no_keyboard(path, person, report)?;
                }
            }
            Err(error) => report.check("open", Outcome::Failed, &error.to_string()),
        }
    }

    let software: Vec<String> = Connect::auto()
        .family(Family::Ferrum)
        .find()
        .unwrap_or_default()
        .into_iter()
        .map(|found| found.port)
        .collect();
    let mut ferrums: Vec<(String, FerrumApi)> = software
        .into_iter()
        .map(|port| (port, FerrumApi::Software))
        .collect();
    if ferrums.is_empty() {
        let port = person.answer(
            "\nno Ferrum app port found. if you have a Ferrum, start its app and type the app's port (like COM5), \
             otherwise just press Enter:",
            "",
        )?;
        if !port.is_empty() {
            ferrums.push((port, FerrumApi::Software));
        }
    }
    if !ferrums.is_empty() || person.yes_no("\ndo you have a Ferrum plugged in?")? == Some(true) {
        let port = person.answer(
            "for the Ferrum's own port (the box itself, not the app), type its port from the list at the top of the \
             report, or press Enter to skip it:",
            "",
        )?;
        if !port.is_empty() {
            ferrums.push((port, FerrumApi::Legacy));
        }
    }
    for (path, api) in ferrums {
        report.section(&format!("Ferrum {api:?} on {path}"));
        person.say(&format!("\nFerrum ({api:?}) on {path}"));
        setup(person, report, &[])?;
        match Connect::port(&path)
            .family(Family::Ferrum)
            .ferrum_api(api)
            .open()
        {
            Ok(device) => on_box(
                device,
                person,
                report,
                "the Ferrum's cable to this PC",
                None,
            )?,
            Err(error) => {
                report.check("open", Outcome::Failed, &error.to_string());
                continue;
            }
        }
        ferrum::press_through_close(&path, person, report)?;
    }

    for found in anybox::find().unwrap_or_default() {
        if !matches!(found.info.family, Family::MakcuV4 | Family::Medius) {
            continue;
        }
        let question = format!(
            "\nalso run the checks on the {} on {}? (optional, it's about 10 minutes)",
            found.info.family, found.port
        );
        if person.yes_no(&question)? != Some(true) {
            continue;
        }
        report.section(&format!("{} on {}", found.info.family, found.port));
        setup(person, report, &[])?;
        match Connect::port(&found.port).family(found.info.family).open() {
            Ok(device) => on_box(device, person, report, "the box's cable to this PC", None)?,
            Err(error) => report.check("open", Outcome::Failed, &error.to_string()),
        }
    }

    ports(report, "at the end");
    Ok(())
}

/// the whole common flow on the medius crate's mock box with a script answering, to prove the tester itself
fn dry_run(person: &Person, report: &mut Report) -> Run<()> {
    let found = ports(report, "at the start");
    report.line(&format!(
        "{} ports, none of them used in a dry run",
        found.len()
    ));
    report.section("mock Medius");
    let mock = anybox::medius::MockBox::new();
    let device = match Device::mock_medius(mock.clone(), ClickTiming::default()) {
        Ok(device) => device,
        Err(error) => {
            report.check("open", Outcome::Failed, &error.to_string());
            return Ok(());
        }
    };
    setup(person, report, &[])?;
    // stands in for the person moving and clicking, so the event checks have something to count
    let done = Arc::new(AtomicBool::new(false));
    let input = {
        let (mock, done) = (mock.clone(), done.clone());
        std::thread::spawn(move || {
            use anybox::medius::{Class, Direction, Usage};
            let mut seq = 0u32;
            while !done.load(Ordering::Acquire) {
                seq += 1;
                mock.push_motion(seq as u8, seq * 1000, 3, -2, 0, 0);
                if seq.is_multiple_of(10) {
                    let left = Usage::new(Class::Button, 0);
                    mock.push_usages(
                        seq as u8,
                        seq * 1000,
                        Class::Button,
                        Direction::PRESS,
                        &[left],
                    );
                    mock.push_usages(
                        seq as u8,
                        seq * 1000 + 1,
                        Class::Button,
                        Direction::RELEASE,
                        &[],
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        })
    };
    let restart = || mock.restart();
    let ran = on_box(device, person, report, "the mock box", Some(&restart));
    done.store(true, Ordering::Release);
    let _ = input.join();
    ran
}
