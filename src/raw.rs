//! counts every relative mouse report windows gets, wherever the pointer is. egui drops motion once the pointer
//! leaves the window (or sits on the taskbar), so a move bigger than the window never counts whole there. this
//! runs as its own process because windows only gives a process one raw input target for mice, and the window's
//! event loop already took it

use std::io::{self, BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};

pub const FLAG: &str = "--raw-motion";

// the tester is a windows app, so without this the helper would pop a console window
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Counter {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Counter {
    pub fn start() -> io::Result<Self> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg(FLAG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
        let input = child.stdin.take().expect("piped");
        let output = child.stdout.take().expect("piped");
        let mut counter = Self {
            child,
            input,
            output: BufReader::new(output),
        };
        let mut line = String::new();
        counter.output.read_line(&mut line)?;
        if line.trim() != "ready" {
            return Err(io::Error::other(format!(
                "the raw input helper said {:?} instead of ready",
                line.trim()
            )));
        }
        Ok(counter)
    }

    /// what came in since the last call
    pub fn take(&mut self) -> io::Result<(i64, i64)> {
        writeln!(self.input)?;
        self.input.flush()?;
        let mut line = String::new();
        self.output.read_line(&mut line)?;
        let mut parts = line.split_whitespace().map(str::parse::<i64>);
        match (parts.next(), parts.next()) {
            (Some(Ok(x)), Some(Ok(y))) => Ok((x, y)),
            _ => Err(io::Error::other(format!(
                "the raw input helper said {:?}",
                line.trim()
            ))),
        }
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static X: AtomicI64 = AtomicI64::new(0);
static Y: AtomicI64 = AtomicI64::new(0);

/// the helper side. every line on stdin gets the counts since the last one back, and stdin closing ends it
pub fn serve() {
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HID_USAGE_GENERIC_MOUSE, HID_USAGE_PAGE_GENERIC,
    };
    use windows_sys::Win32::UI::Input::{
        GetRawInputData, HRAWINPUT, MOUSE_MOVE_ABSOLUTE, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
        RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEMOUSE, RegisterRawInputDevices,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG, WM_INPUT,
    };

    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    // a message only window is enough to get WM_INPUT, and INPUTSINK keeps it coming without focus
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    let device = RAWINPUTDEVICE {
        usUsagePage: HID_USAGE_PAGE_GENERIC,
        usUsage: HID_USAGE_GENERIC_MOUSE,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: window,
    };
    let registered = !window.is_null()
        && unsafe { RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0 };
    if !registered {
        println!(
            "couldn't register for raw input: {}",
            io::Error::last_os_error()
        );
        return;
    }
    println!("ready");
    std::thread::spawn(|| {
        for line in io::stdin().lock().lines() {
            if line.is_err() {
                break;
            }
            println!(
                "{} {}",
                X.swap(0, Ordering::Relaxed),
                Y.swap(0, Ordering::Relaxed)
            );
        }
        std::process::exit(0);
    });
    let mut message: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        if message.message == WM_INPUT {
            let mut raw: RAWINPUT = unsafe { std::mem::zeroed() };
            let mut size = size_of::<RAWINPUT>() as u32;
            let read = unsafe {
                GetRawInputData(
                    message.lParam as HRAWINPUT,
                    RID_INPUT,
                    (&raw mut raw).cast(),
                    &mut size,
                    size_of::<RAWINPUTHEADER>() as u32,
                )
            };
            if read != u32::MAX && raw.header.dwType == RIM_TYPEMOUSE {
                let mouse = unsafe { raw.data.mouse };
                if mouse.usFlags & MOUSE_MOVE_ABSOLUTE == 0 {
                    X.fetch_add(i64::from(mouse.lLastX), Ordering::Relaxed);
                    Y.fetch_add(i64::from(mouse.lLastY), Ordering::Relaxed);
                }
            }
        }
        unsafe { DispatchMessageW(&message) };
    }
}
