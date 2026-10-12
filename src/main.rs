// no console window next to the tester's own window
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod checks;
mod ferrum;
mod gui;
mod pad;
mod person;
mod report;
mod ysc;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anybox::{ClickTiming, Connect, Device, Family, FerrumApi};
use serialport::SerialPortType;

use checks::{Checks, closing};
use person::{Person, Run, STOP, Ui};
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

    let stamp = report::now() as u64;
    let trace = out.join(format!("anybox-trace-{stamp}.tmp"));
    // SAFETY: nothing else runs yet, so no other thread reads the environment while it changes
    unsafe { std::env::set_var("ANYBOX_TRACE", &trace) };

    if scripted {
        let _ = ctrlc::set_handler(|| STOP.store(true, Ordering::Release));
        let person = Person { ui: None };
        let (failed, said) = session(&person, &out, stamp, &trace);
        println!("{said}");
        if failed > 0 {
            std::process::exit(1);
        }
        return;
    }

    let ui = Ui::new();
    let worker = {
        let ui = ui.clone();
        std::thread::spawn(move || {
            let person = Person {
                ui: Some(ui.clone()),
            };
            let (_, said) = session(&person, &out, stamp, &trace);
            ui.change(|view| view.done = Some(said));
        })
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("anybox tester")
            .with_maximized(true),
        ..Default::default()
    };
    let started = eframe::run_native(
        "anybox tester",
        options,
        Box::new(move |cc| {
            let _ = ui.ctx.set(cc.egui_ctx.clone());
            Ok(Box::new(gui::App::new(ui, worker)))
        }),
    );
    if let Err(error) = started {
        eprintln!("couldn't open the window: {error}");
    }
}

/// the whole run and its report. how many checks failed, and where the report went
fn session(person: &Person, out: &Path, stamp: u64, trace: &Path) -> (usize, String) {
    let mut report = Report::default();
    let ran = if person.scripted() {
        dry_run(person, &mut report)
    } else {
        run(person, &mut report)
    };
    if ran.is_err() {
        report.line(
            "
the run was stopped before the end",
        );
    }

    // every device and probe is closed by now, so the trace is flushed
    let traced = std::fs::read_to_string(trace).unwrap_or_default();
    let _ = std::fs::remove_file(trace);
    let header = format!(
        "anybox-testing {}, anybox {}, {}",
        env!("CARGO_PKG_VERSION"),
        env!("ANYBOX_VERSION"),
        windows_version()
    );
    let name = if person.scripted() {
        "anybox-dry-run"
    } else {
        "anybox-report"
    };
    let file = out.join(format!("{name}-{stamp}.txt"));
    let said = match std::fs::write(&file, report.render(&header, &traced)) {
        Ok(()) => file.display().to_string(),
        Err(error) => format!("couldn't write the report to {}: {error}", file.display()),
    };
    (report.failed(), said)
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
fn ports(person: &Person, report: &mut Report, when: &str) -> Vec<serialport::SerialPortInfo> {
    report.section(&format!("ports {when}"));
    person.say("");
    person.say(&format!("serial ports {when}:"));
    let ports = serialport::available_ports().unwrap_or_default();
    if ports.is_empty() {
        person.say("  none");
        report.line("none");
    }
    for port in &ports {
        let line = match &port.port_type {
            SerialPortType::UsbPort(usb) => format!(
                "{:<7} USB {:04X}:{:04X}  serial {}  {} / {}",
                port.port_name,
                usb.vid,
                usb.pid,
                usb.serial_number.as_deref().unwrap_or("none"),
                usb.manufacturer.as_deref().unwrap_or("?"),
                usb.product.as_deref().unwrap_or("?"),
            ),
            other => format!("{:<7} {other:?}", port.port_name),
        };
        person.say(&format!("  {line}"));
        report.line(&line);
    }
    ports
}

fn is_ysc_left(port: &serialport::SerialPortInfo) -> bool {
    matches!(&port.port_type, SerialPortType::UsbPort(usb)
        if (usb.vid, usb.pid) == (0x1A86, 0xFE0C) && usb.serial_number.as_deref() == Some(YSC_LEFT))
}

/// the questions about what's plugged in, since anybox can't read any of it
fn setup(person: &Person, report: &mut Report, extra: &[(&str, &str)]) -> Run<()> {
    let mut questions = vec![
        (
            "which mouse is plugged into the box, and at what polling rate?",
            "roughly is fine, like \"G Pro Superlight, 1000 Hz\"",
        ),
        (
            "which keyboard is plugged into the box, and at what polling rate?",
            "roughly is fine too, or \"none\"",
        ),
    ];
    questions.extend_from_slice(extra);
    for (question, how) in questions {
        let answer = person.answer(question, how, "dry run")?;
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
    checks.reconnect(what, unplug)?;
    checks.hold_for_close()
}

/// runs the checks and always closes, so a stopped run still lets go of everything
fn on_box(
    device: Device,
    person: &Person,
    report: &mut Report,
    what: &str,
    unplug: Option<&dyn Fn()>,
) -> Run<()> {
    closing(&device, person, report, |report| {
        common(&device, person, report, what, unplug)
    })
}

fn run(person: &Person, report: &mut Report) -> Run<()> {
    person.heading("anybox tester");
    if !person.ready(
        "this checks how anybox does with your boxes and writes everything into one report you send back.\n\n\
         it moves your cursor a little, clicks right, middle and the side buttons, presses and lets go of left \
         while you're holding it, and holds F13, F14 and shift. it never types anything, and the window on the \
         right sees what arrives, so most checks judge themselves.\n\n\
         before you start:\n\
         - close games and anything that reacts to clicks\n\
         - keep this window in front the whole time\n\
         - if anything ever stays held, unplugging the box lets go of it\n\
         - closing the window stops the run, anybox lets go of everything first",
    )? {
        return Ok(());
    }
    let found = ports(person, report, "at the start");
    let listed: Vec<String> = found
        .iter()
        .map(|port| match &port.port_type {
            SerialPortType::UsbPort(usb) => format!(
                "{} ({})",
                port.port_name,
                usb.product.as_deref().unwrap_or("USB")
            ),
            _ => port.port_name.clone(),
        })
        .collect();

    if !found.iter().any(is_ysc_left) {
        report.line("no YSC 8K command port (1A86:FE0C with serial TOWMCULEFT) found");
    }
    for port in found.iter().filter(|p| is_ysc_left(p)) {
        let path = port.port_name.as_str();
        report.section(&format!("YSC 8K on {path}"));
        person.heading(&format!("YSC 8K on {path}"));
        setup(
            person,
            report,
            &[
                (
                    "which port on the YSC is the cable to this PC plugged into?",
                    "just say what it's labeled, or describe where it is",
                ),
                (
                    "is interpolation or a curve turned on in the YSC tool?",
                    "yes, no, or not sure",
                ),
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
                ports(person, report, &format!("after the YSC on {path}"));
            }
            Err(error) => report.check("open", Outcome::Failed, &error.to_string()),
        }
    }

    report.section("Ferrum search");
    let software = Connect::auto().family(Family::Ferrum).find();
    report.line(&format!("Ferrum app ports anybox found: {software:?}"));
    let software: Vec<String> = software
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
            "no Ferrum app port found. if you have a Ferrum, start its app and type the app's port",
            "like COM5, or just press Enter if you don't have one",
            "",
        )?;
        if !port.is_empty() {
            ferrums.push((port, FerrumApi::Software));
        }
    }
    if !ferrums.is_empty() || person.yes_no("do you have a Ferrum plugged in?")? == Some(true) {
        let port = person.answer(
            &format!(
                "now the Ferrum's own port, the box itself and not the app. the ports on this PC: {}",
                listed.join(", ")
            ),
            "like COM7, or just press Enter to skip it",
            "",
        )?;
        if !port.is_empty() {
            ferrums.push((port, FerrumApi::Legacy));
        }
    }
    for (path, api) in ferrums {
        report.section(&format!("Ferrum {api:?} on {path}"));
        person.heading(&format!("Ferrum ({api:?}) on {path}"));
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
        ports(person, report, &format!("after the Ferrum on {path}"));
    }

    let others = anybox::find();
    if let Err(error) = &others {
        report.line(&format!("looking for other boxes failed: {error}"));
    }
    for found in others.unwrap_or_default() {
        if !matches!(found.info.family, Family::MakcuV4 | Family::Medius) {
            continue;
        }
        let question = format!(
            "also run the checks on the {} on {}? it's optional and takes about 5 minutes",
            found.info.family, found.port
        );
        if person.yes_no(&question)? != Some(true) {
            continue;
        }
        report.section(&format!("{} on {}", found.info.family, found.port));
        person.heading(&format!("{} on {}", found.info.family, found.port));
        setup(person, report, &[])?;
        match Connect::port(&found.port).family(found.info.family).open() {
            Ok(device) => on_box(device, person, report, "the box's cable to this PC", None)?,
            Err(error) => report.check("open", Outcome::Failed, &error.to_string()),
        }
    }

    ports(person, report, "at the end");
    Ok(())
}

/// the whole common flow on the medius crate's mock box with a script answering, to prove the tester itself
fn dry_run(person: &Person, report: &mut Report) -> Run<()> {
    let found = ports(person, report, "at the start");
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
                let wheel = i16::from(seq.is_multiple_of(5));
                mock.push_motion(seq as u8, seq * 1000, 3, -2, wheel, 0);
                if seq.is_multiple_of(4) {
                    // every button in turn, so each one shows up within a short timed part
                    let button = Usage::new(Class::Button, ((seq / 4) % 5) as u16);
                    mock.push_usages(
                        seq as u8,
                        seq * 1000,
                        Class::Button,
                        Direction::PRESS,
                        &[button],
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
