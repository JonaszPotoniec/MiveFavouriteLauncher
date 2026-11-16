# Mive favourite launcher

A 3×4 app grid for the Mive K-Suma's favourite key. Set it as the phone's favourite app, then a press of the favourite key shows 12 apps laid out like the keypad, and pressing `1`–`9`, `*`, `0` or `#` starts the app on that key.

It is a native Rust app (no Java or Kotlin), so it shows the grid about 0.15 s after the key press when it is already running, and about 0.3–0.4 s from a fresh start.

## Using it

- **Number keys:** start that tile's app. On an empty tile, open the app picker.
- **D-pad:** move the highlight. **OK** starts the highlighted app; **hold OK** or press **Menu** to change it.
- **Touch:** tap starts, long-press changes.
- **Back:** hides the launcher (it keeps running, so the next press is fast).
- **App picker:** Up/Down to move, Left/Right for a page, OK to assign, Back to cancel. The first row empties the tile.

## Install

Needs JDK 17+, Android SDK platform 37, NDK `30.0.16248370`, Rust 1.100+ with the `armv7-linux-androideabi` target, and `cargo-ndk`:

```bash
rustup target add armv7-linux-androideabi
```

```bash
cargo install cargo-ndk
```

Build and install (Gradle runs cargo-ndk for you):

```bash
./gradlew installRelease
```

Then hold the phone's favourite key and choose **Favourites**.

## Development

```bash
cargo test -p mivefav-core
```

```bash
cargo run -p mivefav-core --example preview -- /path/to/RobotoStatic-Regular.ttf /tmp
```

The preview writes `grid.png` and `picker.png` at the phone's size. `rust/core` holds layout, drawing and storage and builds on any host; `rust/app` is the Android glue.
