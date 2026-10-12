use anybox::{Connect, Device, Family, FerrumApi};
use serialport::{SerialPortInfo, SerialPortType};

use crate::person::{Answer, Person, Phase, Run};
use crate::report::{Outcome, Report};

#[derive(Clone, Debug)]
pub struct Setup {
    pub family: Family,
    pub keyboard: bool,
    pub can_position: bool,
    pub name: String,
    pub khz: bool,
    pub rate: String,
    pub connector: String,
    pub curves: String,
    pub ferrum_legacy: bool,
    pub legacy_port: String,
    pub ysc_keys: bool,
    pub ysc_debug: bool,
    /// nothing is picked until the person picks it, a default box would connect to the wrong one unnoticed
    pub picked: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            family: Family::Medius,
            keyboard: false,
            can_position: true,
            name: String::new(),
            khz: false,
            rate: String::new(),
            connector: String::new(),
            curves: "not sure".into(),
            ferrum_legacy: false,
            legacy_port: String::new(),
            ysc_keys: false,
            ysc_debug: false,
            picked: false,
        }
    }
}

pub fn form(person: &Person, report: &mut Report) -> Run<Setup> {
    let setup = if person.scripted() {
        Setup {
            name: "mock mouse".into(),
            ..Setup::default()
        }
    } else {
        match person.ask(Phase::Setup, "set up the test", "")? {
            Answer::Setup(setup) => setup,
            _ => return Err(crate::person::Stopped),
        }
    };
    report.section("setup");
    report.line(&format!(
        "box {}, attached {}, device {}, declared rate {}",
        setup.family,
        if setup.keyboard { "keyboard" } else { "mouse" },
        setup.name,
        if setup.khz {
            "1000 hz"
        } else if setup.rate.is_empty() {
            "not sure"
        } else {
            &setup.rate
        }
    ));
    if setup.family == Family::Ysc {
        report.line(&format!(
            "pc connector {}, curves or interpolation {} (person's settings, not read back)",
            setup.connector, setup.curves
        ));
    }
    report.line("other attached devices and 125/1000/8000 hz configurations not checked");
    report.line("curves readback, unique unit id, signed-lock readback, cmd 46 acknowledgments, and km.buttons format not checked");
    if let Some(ui) = &person.ui {
        ui.change(|v| {
            v.keyboard = setup.keyboard;
            v.can_position = setup.can_position;
            v.require_area = setup.can_position;
        });
    }
    Ok(setup)
}

pub fn ports(person: &Person, report: &mut Report, when: &str) -> Vec<SerialPortInfo> {
    report.section(&format!("ports {when}"));
    let ports = match serialport::available_ports() {
        Ok(ports) => ports,
        Err(error) => {
            report.line(&format!("couldn't list ports, {error}"));
            return Vec::new();
        }
    };
    if ports.is_empty() {
        report.line("none");
    }
    for port in &ports {
        let line = match &port.port_type {
            SerialPortType::UsbPort(usb) => format!(
                "{} USB {:04X}:{:04X} serial {} {}",
                port.port_name,
                usb.vid,
                usb.pid,
                usb.serial_number.as_deref().unwrap_or("none"),
                usb.product.as_deref().unwrap_or("?")
            ),
            other => format!("{} {other:?}", port.port_name),
        };
        report.line(&line);
        person.say(&line);
    }
    ports
}

pub fn is_ysc_left(port: &SerialPortInfo) -> bool {
    matches!(&port.port_type, SerialPortType::UsbPort(usb)
        if (usb.vid, usb.pid) == (0x1A86, 0xFE0C) && usb.serial_number.as_deref() == Some("TOWMCULEFT"))
}

pub fn connect(
    setup: &Setup,
    person: &Person,
    report: &mut Report,
) -> Run<Option<(Device, String)>> {
    loop {
        if let Some(ui) = &person.ui {
            ui.change(|v| {
                v.phase = Phase::Connecting;
                v.text = "looking for the selected box".into();
            });
        }
        let listed = ports(person, report, "before open");
        let candidates: Vec<String> = if setup.family == Family::Ysc {
            listed
                .iter()
                .filter(|p| is_ysc_left(p))
                .map(|p| p.port_name.clone())
                .collect()
        } else {
            let found = if setup.family == Family::Ferrum {
                Connect::auto().family(Family::Ferrum).find()
            } else {
                anybox::find()
            };
            match found {
                Ok(found) => found
                    .into_iter()
                    .filter(|p| p.info.family == setup.family)
                    .map(|p| p.port)
                    .collect(),
                Err(error) => {
                    report.line(&format!("discovery failed, {error}"));
                    Vec::new()
                }
            }
        };
        let path = if candidates.len() == 1 {
            candidates[0].clone()
        } else {
            let prompt = if candidates.is_empty() {
                "no matching port yet"
            } else {
                "which port?"
            };
            let hint = if setup.family == Family::Ysc {
                "only the guarded left port is allowed, leave empty to scan again"
            } else {
                "choose a listed port or type its COM name, leave empty to scan again"
            };
            let Some(ui) = &person.ui else {
                return Ok(None);
            };
            ui.change(|v| v.status = candidates.clone());
            match person.ask(Phase::Text, prompt, hint)? {
                Answer::Text(s) => s.trim().to_string(),
                Answer::Skip => return Ok(None),
                _ => String::new(),
            }
        };
        if path.is_empty() {
            continue;
        }
        if setup.family == Family::Ysc
            && !listed.iter().any(|p| p.port_name == path && is_ysc_left(p))
        {
            report.check(
                "port guard",
                Outcome::Noted,
                "the selected port is not the YSC left command port",
            );
            person.answer("that isn't the ysc left port, scan again", "", "")?;
            continue;
        }
        match Connect::port(&path)
            .family(setup.family)
            .ferrum_api(FerrumApi::Software)
            .open()
        {
            Ok(device) => return Ok(Some((device, path))),
            Err(error) => {
                report.check("open", Outcome::Failed, &error.to_string());
                if !matches!(
                    person.ask(
                        Phase::Card,
                        &format!("couldn't open it, {error}"),
                        "start to scan again, skip to save the report"
                    )?,
                    Answer::Start
                ) {
                    return Ok(None);
                }
            }
        }
    }
}
