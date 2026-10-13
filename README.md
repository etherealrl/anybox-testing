# anybox-testing

checks a supported passthrough box through anybox and saves a plain text report, with a window that tells you what to do one step at a time

## running it

grab `anybox-testing.exe` from the latest release and run it. the exe is built by GitHub Actions straight from the code in this repo (`.github/workflows/release.yml`), so what you run is exactly what you can read here

or build it yourself with Rust from <https://rustup.rs>:

```sh
cargo run --release
```

pick what you're testing from the list (the box, the way into it on a Ferrum, and whether a mouse or a keyboard is plugged into it), then the device name and its rate if you know it. close games and keep the window in front, because the tests send clicks and keys

the tests cover output, physical input, locks, event receivers, reconnect, and close. the extra ysc checks are on by default under "extra checks". a ysc V2 is found on its left port, and when there's no V2 a CH343 port gets tried as a V1. the window tells you when to unplug the box's cable to this pc

if anything stays held or locked, unplug the box and plug it back in. stop run or closing the window lets the worker close the connection and save first

## the report

the report goes next to the exe by default, or into the folder you pass with `--out`. it includes the checks, port details, and full serial trace, so read it before you share it because the trace can include unrelated typing

a recorded result is an observation, it doesn't establish that the box passed a hardware check. missing capabilities and physical controls stay in the report as not available

## dry run

```sh
cargo run -- --dry-run --out reports
```

this uses a mock Medius and scripted mouse input, without opening a window or driving a real box. checks that need the window are recorded as `no window in a dry run`
