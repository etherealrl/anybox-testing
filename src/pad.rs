use std::time::{Duration, Instant};

use anybox::Button;

/// what Windows delivered to the tester's window, so a check sees whether anybox's output actually reached the PC
#[derive(Clone, Debug)]
pub enum Input {
    Button {
        button: Button,
        pressed: bool,
    },
    /// positive is up, like anybox's wheel
    Wheel(f32),
    /// raw mouse counts, before Windows' acceleration and the screen edges
    Moved(f32, f32),
    Key {
        key: String,
        pressed: bool,
    },
    Text(String),
}

#[derive(Clone, Debug)]
pub struct Seen {
    pub at: Instant,
    pub input: Input,
}

pub fn presses(seen: &[Seen], button: Button) -> usize {
    seen.iter()
        .filter(|s| matches!(s.input, Input::Button { button: b, pressed: true } if b == button))
        .count()
}

pub fn releases(seen: &[Seen], button: Button) -> usize {
    seen.iter()
        .filter(|s| matches!(s.input, Input::Button { button: b, pressed: false } if b == button))
        .count()
}

/// whether the button was down at the end, going by the last press or release seen
pub fn ends_down(seen: &[Seen], button: Button) -> Option<bool> {
    seen.iter().rev().find_map(|s| match s.input {
        Input::Button { button: b, pressed } if b == button => Some(pressed),
        _ => None,
    })
}

/// how long the first press of the button was held
pub fn first_hold(seen: &[Seen], button: Button) -> Option<Duration> {
    let down = seen.iter().position(
        |s| matches!(s.input, Input::Button { button: b, pressed: true } if b == button),
    )?;
    let up = seen[down..]
        .iter()
        .find(|s| matches!(s.input, Input::Button { button: b, pressed: false } if b == button))?;
    Some(up.at - seen[down].at)
}

pub fn moved(seen: &[Seen]) -> (f32, f32) {
    seen.iter().fold((0.0, 0.0), |(x, y), s| match s.input {
        Input::Moved(dx, dy) => (x + dx, y + dy),
        _ => (x, y),
    })
}

pub fn wheel(seen: &[Seen]) -> f32 {
    seen.iter()
        .map(|s| match s.input {
            Input::Wheel(notches) => notches,
            _ => 0.0,
        })
        .sum()
}

pub fn key_down(seen: &[Seen], key: &str) -> bool {
    seen.iter()
        .any(|s| matches!(&s.input, Input::Key { key: k, pressed: true } if k == key))
}

pub fn text(seen: &[Seen]) -> String {
    seen.iter()
        .filter_map(|s| match &s.input {
            Input::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// the buttons in order, like "Left down, Left up", short enough for one report line
pub fn buttons(seen: &[Seen]) -> String {
    let order: Vec<String> = seen
        .iter()
        .filter_map(|s| match s.input {
            Input::Button { button, pressed } => Some(format!(
                "{button:?} {}",
                if pressed { "down" } else { "up" }
            )),
            _ => None,
        })
        .collect();
    if order.is_empty() {
        "no clicks".to_string()
    } else {
        order.join(", ")
    }
}

pub fn summary(seen: &[Seen]) -> String {
    let (dx, dy) = moved(seen);
    let keys: Vec<&str> = seen
        .iter()
        .filter_map(|s| match &s.input {
            Input::Key { key, pressed: true } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    format!(
        "the window saw: {}, moved {dx:.0} {dy:.0}, wheel {:.0}, keys {keys:?}, text {:?}",
        buttons(seen),
        wheel(seen),
        text(seen)
    )
}

#[cfg(test)]
mod tests;
