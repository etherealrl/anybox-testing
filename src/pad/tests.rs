use std::time::{Duration, Instant};

use anybox::Button;

use super::{Input, Seen, ends_down, first_hold, moved, presses, releases, text, wheel};

fn at(start: Instant, millis: u64, input: Input) -> Seen {
    Seen {
        at: start + Duration::from_millis(millis),
        input,
    }
}

fn click(button: Button, pressed: bool) -> Input {
    Input::Button { button, pressed }
}

#[test]
fn a_forced_release_then_the_held_button_coming_back_ends_down() {
    let start = Instant::now();
    let seen = [
        at(start, 0, click(Button::Left, true)),
        at(start, 500, click(Button::Left, false)),
        at(start, 560, click(Button::Left, true)),
        at(start, 600, Input::Moved(3.0, -1.0)),
    ];
    assert_eq!(presses(&seen, Button::Left), 2);
    assert_eq!(releases(&seen, Button::Left), 1);
    assert_eq!(ends_down(&seen, Button::Left), Some(true));
    assert_eq!(ends_down(&seen, Button::Right), None);
    assert_eq!(
        first_hold(&seen, Button::Left),
        Some(Duration::from_millis(500))
    );
    assert_eq!(moved(&seen), (3.0, -1.0));
}

#[test]
fn wheel_and_text_add_up() {
    let start = Instant::now();
    let seen = [
        at(start, 0, Input::Wheel(1.0)),
        at(start, 1, Input::Wheel(-3.0)),
        at(start, 2, Input::Text("A".into())),
        at(start, 3, Input::Text("BC".into())),
    ];
    assert_eq!(wheel(&seen), -2.0);
    assert_eq!(text(&seen), "ABC");
}
