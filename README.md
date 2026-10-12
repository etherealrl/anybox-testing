# anybox-testing

checks a supported passthrough box through anybox and saves a plain text report, with a window that tells you what to do one step at a time

## running it

build it with Rust from <https://rustup.rs>:

```sh
cargo run --release
```

choose the box, the one mouse or keyboard plugged into it, and its rate if you know it. close games and keep the window in front, because the tests send clicks and keys

the tests cover output, physical input, locks, event receivers, reconnect, and close. the ysc and ferrum diagnostics are optional, and the window tells you when to unplug the box's cable to this pc

if anything stays held or locked, unplug the box and plug it back in. stop run or closing the window lets the worker close the connection and save first

## the report

the report goes next to the exe by default, or into the folder you pass with `--out`. it includes the checks, port details, and full serial trace, so read it before you share it because the trace can include unrelated typing

a recorded result is an observation, it doesn't establish that the box passed a hardware check. missing capabilities and physical controls stay in the report as not available

## dry run

```sh
cargo run -- --dry-run --out reports
```

this uses a mock Medius and scripted mouse input, without opening a window or driving a real box. checks that need the window are recorded as `no window in a dry run`
