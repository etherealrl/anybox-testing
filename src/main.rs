// no console window next to the tester's own window
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod checks;
mod ferrum;
mod gui;
mod pad;
mod person;
mod report;
mod setup;
mod ysc;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anybox::{ClickTiming, Connect, Device, Family, FerrumApi};
use setup::ports;

use checks::{Checks, closing};
use person::{Person, Run, STOP, Ui};
use report::{Outcome, Report};

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
            "--out" => match args.next() {
                Some(folder) => out = PathBuf::from(folder),
                None => {
                    eprintln!("--out needs a folder");
                    std::process::exit(2);
                }
            },
            other => {
                eprintln!("unknown option {other}, the only ones are --dry-run and --out <folder>");
                std::process::exit(2);
            }
        }
    }

    let stamp = report::now() as u64;
    let trace = out.join(format!("anybox-trace-{stamp}.tmp"));
    let _ = std::fs::create_dir_all(&out);
    // safety: no other thread reads the environment yet
    unsafe { std::env::set_var("ANYBOX_TRACE", &trace) };

    if scripted {
        let _ = ctrlc::set_handler(|| STOP.store(true, Ordering::Release));
        let person = Person { ui: None };
        let (failed, said) = session(&person, &out, stamp, &trace);
        match &said {
            Ok(path) => println!("{path}"),
            Err(error) => eprintln!("{error}"),
        }
        if failed > 0 || said.is_err() {
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
            ui.change(|view| {
                view.phase = person::Phase::Finished;
                view.done = Some(said);
            });
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
            // a test tool read from arm's length, the default text is too small for that
            cc.egui_ctx.set_zoom_factor(1.3);
            let _ = ui.ctx.set(cc.egui_ctx.clone());
            Ok(Box::new(gui::App::new(ui, worker)))
        }),
    );
    if let Err(error) = started {
        eprintln!("couldn't open the window: {error}");
    }
}
fn session(
    person: &Person,
    out: &Path,
    stamp: u64,
    trace: &Path,
) -> (usize, Result<String, String>) {
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
    let rendered = report.render(&header, &traced);
    let summary = rendered
        .lines()
        .nth(1)
        .unwrap_or_default()
        .replace("noted", "recorded");
    let mut folder = out.to_path_buf();
    let said = loop {
        let file = folder.join(format!("{name}-{stamp}.txt"));
        match std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&file, &rendered)) {
            Ok(()) => break Ok(file.display().to_string()),
            Err(error) => {
                let message = format!("couldn't write the report, {error}");
                if person.scripted() {
                    break Err(message);
                }
                match person.ask(
                    person::Phase::Save,
                    &message,
                    "the report is still here, choose another folder",
                ) {
                    Ok(person::Answer::Text(path)) if !path.trim().is_empty() => {
                        folder = PathBuf::from(path.trim())
                    }
                    Ok(person::Answer::Text(_)) => {}
                    _ => break Err(message),
                }
            }
        }
    };
    if let Some(ui) = &person.ui {
        ui.change(|v| {
            v.text = "all done".into();
            v.hint = summary;
            v.timer = None;
        });
    }
    (report.failed(), said)
}

fn windows_version() -> String {
    std::process::Command::new("cmd")
        .args(["/c", "ver"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|_| std::env::consts::OS.to_string())
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
    if let Some(ui) = &person.ui {
        let mut names = vec!["connection evidence", "automatic output"];
        if person.keyboard() {
            names.extend(["physical keys", "key merging"]);
        } else {
            names.extend([
                "physical buttons",
                "physical movement",
                "physical wheel",
                "fast movement",
                "held button, press and release",
                "held button, release only",
                "injection in events",
                "button locks",
                "x lock",
                "y lock",
            ]);
        }
        names.extend(["receiver filtering", "boundary moves", "reconnect", "close"]);
        ui.change(|v| {
            v.cards = names
                .iter()
                .map(|title| person::Card {
                    title: (*title).into(),
                    result: "waiting".into(),
                })
                .collect()
        });
        ui.change(|v| v.cards[0].result = "recorded".into());
    }
    checks.identity();
    checks.card("automatic output", Checks::automatic)?;
    checks.with_them()?;
    checks.card("boundary moves", Checks::boundary)?;
    checks.card("reconnect", |checks| checks.reconnect(what, unplug))?;
    if person
        .ui
        .as_ref()
        .is_some_and(|ui| ui.view.lock().unwrap().reconnect_failed)
    {
        checks
            .report
            .check("held-input close", Outcome::Skipped, "reconnect failed");
        return Ok(());
    }
    checks.hold_for_close()
}
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
    let setup = setup::form(person, report)?;
    let Some((device, path)) = setup::connect(&setup, person, report)? else {
        return Ok(());
    };
    person.heading(&format!("{} on {path}", setup.family));
    on_box(device, person, report, "the box's cable to this pc", None)?;
    if setup.family == Family::Ferrum && setup.ferrum_legacy && !setup.legacy_port.is_empty() {
        let path = &setup.legacy_port;
        report.section(&format!("Ferrum legacy on {path}"));
        match Connect::port(path)
            .family(Family::Ferrum)
            .ferrum_api(FerrumApi::Legacy)
            .open()
        {
            Ok(device) => on_box(device, person, report, "the box's cable to this pc", None)?,
            Err(error) => report.check("legacy open", Outcome::Failed, &error.to_string()),
        }
        ferrum::press_through_close(path, person, report)?;
    }
    if setup.family == Family::Ysc {
        if setup.ysc_keys {
            if !setup.keyboard || person.confirm("unplug the keyboard from the box now")? {
                ysc::no_keyboard(&path, person, report)?;
            } else {
                report.check(
                    "no keyboard diagnostic",
                    Outcome::Skipped,
                    "keyboard removal wasn't confirmed",
                );
            }
        } else {
            report.check("no keyboard diagnostic", Outcome::Skipped, "");
        }
        if setup.ysc_debug {
            ysc::debug_mode(&path, person, report)?;
        } else {
            report.check("debug mode diagnostic", Outcome::Skipped, "");
        }
    }
    ports(person, report, "at the end");
    Ok(())
}
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
    setup::form(person, report)?;
    // stands in for the person moving and clicking, so the event checks have something to count
    let done = Arc::new(AtomicBool::new(false));
    let input = {
        let (mock, done) = (mock.clone(), done.clone());
        std::thread::spawn(move || {
            use anybox::medius::{Class, Direction, Usage};
            let mut seq = 0u32;
            while !done.load(Ordering::Acquire) {
                seq += 1;
                let wheel = if seq.is_multiple_of(10) {
                    -1
                } else {
                    i16::from(seq.is_multiple_of(5))
                };
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
