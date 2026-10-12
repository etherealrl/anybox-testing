use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Ctrl+C, so every wait can stop and the devices get closed on the way out instead of left holding something
pub static STOP: AtomicBool = AtomicBool::new(false);

/// the run was stopped with Ctrl+C or the console closed
#[derive(Debug)]
pub struct Stopped;

pub type Run<T> = Result<T, Stopped>;

/// fits a default console window
const WIDTH: usize = 100;

/// the person at the PC. in a dry run a script answers for them and every wait is a lot shorter
pub struct Person {
    pub scripted: bool,
}

impl Person {
    /// wrapped at a word, a console otherwise breaks lines in the middle of one
    pub fn say(&self, text: &str) {
        for line in text.lines() {
            if line.len() <= WIDTH {
                println!("{line}");
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            let mut out = String::new();
            for word in line.split_whitespace() {
                if !out.is_empty() && out.len() + 1 + word.len() > WIDTH {
                    println!("{out}");
                    out.clear();
                }
                if out.is_empty() {
                    out.push_str(&line[..indent]);
                } else {
                    out.push(' ');
                }
                out.push_str(word);
            }
            println!("{out}");
        }
    }

    /// the start of a box's part
    pub fn heading(&self, title: &str) {
        println!(
            "

=== {title} ==="
        );
    }

    fn read(&self) -> Run<String> {
        if STOP.load(Ordering::Acquire) {
            return Err(Stopped);
        }
        let mut line = String::new();
        // Ctrl+C on Windows ends the read with nothing, same as a closed console
        match io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => Err(Stopped),
            _ if STOP.load(Ordering::Acquire) => Err(Stopped),
            _ => Ok(line.trim().to_string()),
        }
    }

    /// a question, what they can answer, then the cursor on its own line
    fn prompt(&self, text: &str, how: &str) {
        println!();
        self.say(text.trim());
        if !how.is_empty() {
            self.say(&format!("  ({how})"));
        }
        print!("> ");
        let _ = io::stdout().flush();
    }

    /// false when they typed s to skip it
    pub fn ready(&self, text: &str) -> Run<bool> {
        self.prompt(
            text,
            "press Enter to start, or type s and Enter to skip this one",
        );
        if self.scripted {
            println!();
            return Ok(true);
        }
        Ok(!self.read()?.eq_ignore_ascii_case("s"))
    }

    /// none when they skipped it
    pub fn yes_no(&self, question: &str) -> Run<Option<bool>> {
        loop {
            self.prompt(question, "y or n, or s to skip");
            if self.scripted {
                println!("y");
                return Ok(Some(true));
            }
            match self.read()?.to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(Some(true)),
                "n" | "no" => return Ok(Some(false)),
                "s" => return Ok(None),
                _ => self.say("just y, n or s please"),
            }
        }
    }

    pub fn answer(&self, question: &str, how: &str, scripted: &str) -> Run<String> {
        self.prompt(question, how);
        if self.scripted {
            println!("{scripted}");
            return Ok(scripted.to_string());
        }
        self.read()
    }

    /// a timed part starts here, one high beep. it says so on screen too, for a PC without sound
    pub fn go(&self) {
        self.say(">>> NOW");
        if !self.scripted {
            beep(1500, 300);
        }
    }

    /// and ends here, two low beeps
    pub fn stop(&self) {
        self.say(">>> done");
        if !self.scripted {
            beep(500, 250);
            std::thread::sleep(Duration::from_millis(100));
            beep(500, 250);
        }
    }

    pub fn stopped(&self) -> Run<()> {
        if STOP.load(Ordering::Acquire) {
            Err(Stopped)
        } else {
            Ok(())
        }
    }

    /// a dry run waits a twentieth of the time, the mock box needs no time to react
    pub fn scaled(&self, wait: Duration) -> Duration {
        if self.scripted { wait / 20 } else { wait }
    }

    pub fn wait(&self, wait: Duration) -> Run<()> {
        let until = Instant::now() + self.scaled(wait);
        while Instant::now() < until {
            if STOP.load(Ordering::Acquire) {
                return Err(Stopped);
            }
            std::thread::sleep(
                Duration::from_millis(20).min(until.saturating_duration_since(Instant::now())),
            );
        }
        Ok(())
    }
}

// the terminal's bell is silent in a lot of terminals, Beep goes through the sound card
#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn Beep(frequency: u32, millis: u32) -> i32;
}

fn beep(frequency: u32, millis: u32) {
    #[cfg(windows)]
    // SAFETY: Beep takes two plain numbers
    unsafe {
        Beep(frequency, millis);
    }
    #[cfg(not(windows))]
    let _ = (frequency, millis);
}
