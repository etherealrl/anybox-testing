use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Ctrl+C, so every wait can stop and the devices get closed on the way out instead of left holding something
pub static STOP: AtomicBool = AtomicBool::new(false);

/// the run was stopped with Ctrl+C or the console closed
#[derive(Debug)]
pub struct Stopped;

pub type Run<T> = Result<T, Stopped>;

/// the person at the PC. in a dry run a script answers for them and every wait is a lot shorter
pub struct Person {
    pub scripted: bool,
}

impl Person {
    pub fn say(&self, text: &str) {
        println!("{text}");
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

    fn prompt(&self, text: &str) {
        print!("{text} ");
        let _ = io::stdout().flush();
    }

    /// false when they typed s to skip it
    pub fn ready(&self, text: &str) -> Run<bool> {
        self.say(text);
        self.prompt("press Enter when you're ready, or s and Enter to skip this part");
        if self.scripted {
            println!();
            return Ok(true);
        }
        Ok(!self.read()?.eq_ignore_ascii_case("s"))
    }

    /// none when they skipped it
    pub fn yes_no(&self, question: &str) -> Run<Option<bool>> {
        loop {
            self.prompt(&format!("{question} (y, n, or s to skip)"));
            if self.scripted {
                println!("y");
                return Ok(Some(true));
            }
            match self.read()?.to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(Some(true)),
                "n" | "no" => return Ok(Some(false)),
                "s" => return Ok(None),
                _ => self.say("just y, n or s"),
            }
        }
    }

    pub fn answer(&self, question: &str, scripted: &str) -> Run<String> {
        self.prompt(question);
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
        self.say(">>> stop, done");
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
