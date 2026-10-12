# anybox-testing

checks how anybox does on your Ferrum and YSC 8K V2 and writes everything into one report you send back. it's a small console program, it tells you what to do at each step and takes about 3 to 5 minutes per box

## running it

grab `anybox-testing.exe` from the latest release and run it. the exe is built by GitHub Actions straight from the code in this repo (see `.github/workflows/release.yml`), so you can read exactly what it does before you run it

or build it yourself, you need Rust from <https://rustup.rs>:

```sh
cargo run --release
```

## what it does to your PC

it moves your cursor a little (a small square that ends where it started), clicks right, middle and the side buttons, presses and lets go of left only while you're holding it yourself, and holds F13, F14 and shift. it never types anything and never touches the box's settings, firmware or anything else stored on it

close games and anything that reacts to clicks first. if something ever stays held or locked, unplugging the box lets go of it, and Ctrl+C stops the run at any time

you'll unplug each box once, at the end of its part

## the report

it's a plain text file next to the exe, named `anybox-report-` and a number. read it before you send it if you want, it has the ports on your PC with their USB ids, what each box said, how each check went, and every byte anybox sent to the boxes and got back. there's no user name, PC name or file path from your PC in it
