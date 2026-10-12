use super::*;

#[test]
fn capture_ignores_key_repeats_and_keeps_wheel_units() {
    let ui = Ui::new();
    ui.change(|v| v.phase = Phase::Active);
    let mut app = App::new(ui.clone(), std::thread::spawn(|| {}));
    let ctx = egui::Context::default();
    let mut baseline = ctx.run_ui(
        egui::RawInput {
            events: vec![Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            }],
            ..Default::default()
        },
        |_| {},
    );
    baseline.textures_delta.clear();
    let input = egui::RawInput {
        events: vec![
            Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: egui::Modifiers::default(),
            },
            Event::MouseWheel {
                phase: egui::TouchPhase::Move,
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(2.0, 24.0),
                modifiers: egui::Modifiers::default(),
            },
            Event::Paste("abc".into()),
        ],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |root| app.record(root.ctx()));
    output.textures_delta.clear();
    let pad = ui.pad.lock().unwrap();
    assert!(!pad.iter().any(|s| matches!(s.input, Input::Key { .. })));
    assert!(
        pad.iter()
            .any(|s| matches!(&s.input, Input::WheelDetail(detail) if detail == "Point 2 24"))
    );
    assert!(pad.iter().any(|s| matches!(s.input, Input::Paste)));
}

#[test]
fn setup_input_does_not_enter_the_measurement_buffer() {
    let ui = Ui::new();
    let mut app = App::new(ui.clone(), std::thread::spawn(|| {}));
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        events: vec![Event::Text("abc".into())],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |root| app.record(root.ctx()));
    output.textures_delta.clear();
    assert!(ui.pad.lock().unwrap().is_empty());
}
