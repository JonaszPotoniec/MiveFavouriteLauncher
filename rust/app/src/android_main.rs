//! The NativeActivity entry point: event loop, screens and key handling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use android_activity::input::{InputEvent, KeyAction, KeyEvent, Keycode, MotionAction, MotionEvent};
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use mivefav_core::cache::{self, Fnv};
use mivefav_core::canvas::{Canvas, Rect};
use mivefav_core::icon::Icon;
use mivefav_core::layout::{COLS, ROWS, SLOTS, cell_at};
use mivefav_core::slots::Slots;
use mivefav_core::text::Text;
use mivefav_core::ui::{self, Dp, GridLayout, Picker, PickerLayout, SlotView};
use ndk::hardware_buffer_format::HardwareBufferFormat;
use ndk::native_window::NativeWindow;

use crate::android::Jni;

const OWN_PACKAGE: &str = "eu.potoniec.mivefavouritelauncher";
/// Bump when the grid's look changes, so cached frames are redrawn.
const THEME_VERSION: u64 = 1;
const FONTS: [&str; 3] = [
    "/system/fonts/RobotoStatic-Regular.ttf",
    "/system/fonts/Roboto-Regular.ttf",
    "/system/fonts/DroidSans.ttf",
];
const LONG_PRESS_MS: u128 = 500;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("mivefav"),
    );
    log::info!("start");
    let mut state = State::new(app.clone());
    while !state.quit {
        app.poll_events(None, |event| state.on_event(event));
    }
    log::info!("exit");
}

/// A rendered grid and what it was rendered from.
struct Frame {
    w: usize,
    h: usize,
    key: u64,
    px: Vec<u32>,
}

enum Screen {
    Grid { focus: Option<usize> },
    Picker(Picker),
}

/// Result of the background freshness check: a newer grid, rendered for `slots`.
struct Refreshed {
    slots: String,
    frame: Frame,
}

struct Touch {
    x: f32,
    y: f32,
    at: Instant,
    moved: bool,
    /// Picker scroll position when the finger went down.
    top: usize,
}

struct State {
    app: AndroidApp,
    jni: Jni,
    files: PathBuf,
    window: Option<NativeWindow>,
    dp: Dp,
    slots: Slots,
    frame: Option<Frame>,
    screen: Screen,
    text: Option<Text>,
    icons: HashMap<String, Icon>,
    tx: Sender<Option<Refreshed>>,
    rx: Receiver<Option<Refreshed>>,
    checking: bool,
    /// OK went down and its long press has not fired yet.
    ok_down: Option<bool>,
    touch: Option<Touch>,
    logged_first_frame: bool,
    quit: bool,
}

impl State {
    fn new(app: AndroidApp) -> State {
        let files = app.internal_data_path().unwrap_or_else(|| PathBuf::from(format!("/data/data/{OWN_PACKAGE}/files")));
        let dp = Dp::from_density(app.config().density().unwrap_or(160));
        let slots = Slots::parse(&std::fs::read_to_string(files.join("slots.txt")).unwrap_or_default());
        let (tx, rx) = channel();
        State {
            jni: Jni::new(&app),
            app,
            files,
            window: None,
            dp,
            slots,
            frame: None,
            screen: Screen::Grid { focus: None },
            text: None,
            icons: HashMap::new(),
            tx,
            rx,
            checking: false,
            ok_down: None,
            touch: None,
            logged_first_frame: false,
            quit: false,
        }
    }

    fn on_event(&mut self, event: PollEvent) {
        match event {
            PollEvent::Wake => self.take_refresh(),
            PollEvent::Main(main) => match main {
                MainEvent::InitWindow { .. } => {
                    self.window = self.app.native_window();
                    self.redraw();
                }
                MainEvent::WindowResized { .. } | MainEvent::RedrawNeeded { .. } | MainEvent::ContentRectChanged { .. } => {
                    self.redraw()
                }
                MainEvent::TerminateWindow { .. } => self.window = None,
                MainEvent::InputAvailable => self.drain_input(),
                MainEvent::Resume { .. } => {
                    // Each favourite-key press starts on the grid.
                    self.screen = Screen::Grid { focus: None };
                    self.ok_down = None;
                    self.redraw();
                    self.start_check();
                }
                MainEvent::ConfigChanged { .. } => {
                    self.dp = Dp::from_density(self.app.config().density().unwrap_or(160));
                }
                MainEvent::Destroy => self.quit = true,
                _ => {}
            },
            _ => {}
        }
    }

    // ---------------------------------------------------------------- drawing

    /// Window size and the area below the status bar.
    fn geometry(&self) -> Option<(usize, usize, Rect)> {
        let w = self.window.as_ref()?;
        let (ww, wh) = (w.width().max(0), w.height().max(0));
        let r = self.app.content_rect();
        let area = if r.right > r.left && r.bottom > r.top && r.right <= ww && r.bottom <= wh {
            Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
        } else {
            Rect::new(0, 0, ww, wh)
        };
        (ww > 0 && wh > 0).then_some((ww as usize, wh as usize, area))
    }

    fn redraw(&mut self) {
        let Some((w, h, area)) = self.geometry() else { return };
        match &self.screen {
            Screen::Grid { .. } => self.ensure_frame(w, h, area),
            Screen::Picker(p) => {
                let layout = PickerLayout::new(area, self.dp);
                let wanted: Vec<String> =
                    p.visible(layout.visible_rows()).filter_map(|i| p.items[i].component.clone()).collect();
                for c in wanted {
                    if !self.icons.contains_key(&c)
                        && let Some(icon) = self.jni.icon(&c, layout.icon)
                    {
                        self.icons.insert(c, icon);
                    }
                }
            }
        }
        if self.text.is_none() && matches!(self.screen, Screen::Picker(_)) {
            self.text = load_font();
        }

        let Some(window) = &self.window else { return };
        if let Err(e) = window.set_buffers_geometry(0, 0, Some(HardwareBufferFormat::R8G8B8X8_UNORM)) {
            log::warn!("set_buffers_geometry: {e}");
        }
        let Ok(mut buf) = window.lock(None) else {
            log::warn!("window lock failed");
            return;
        };
        let (bw, bh, stride) = (buf.width(), buf.height(), buf.stride());
        // SAFETY: the buffer is locked until `buf` drops at the end of this function and holds
        // `stride * height` 4-byte pixels (RGBX_8888, set above).
        let px = unsafe { std::slice::from_raw_parts_mut(buf.bits().cast::<u32>(), stride * bh) };
        let mut canvas = Canvas::new(px, bw, bh, stride);
        match &self.screen {
            Screen::Grid { focus } => {
                match &self.frame {
                    Some(f) if (f.w, f.h) == (bw, bh) => canvas.copy_from_packed(&f.px),
                    _ => canvas.fill(ui::BG),
                }
                if let Some(i) = focus {
                    ui::draw_focus(&mut canvas, &GridLayout::new(area, self.dp), *i);
                }
            }
            Screen::Picker(p) => {
                let layout = PickerLayout::new(area, self.dp);
                match &mut self.text {
                    Some(text) => ui::draw_picker(&mut canvas, &layout, text, p, |c| self.icons.get(c)),
                    None => canvas.fill(ui::BG),
                }
            }
        }
        drop(buf);
        if !self.logged_first_frame {
            self.logged_first_frame = true;
            log::info!("first frame posted");
        }
    }

    /// Makes sure `self.frame` is a grid for this window size: from memory, from the cache
    /// file, or rendered now.
    fn ensure_frame(&mut self, w: usize, h: usize, area: Rect) {
        if self.frame.as_ref().is_some_and(|f| (f.w, f.h) == (w, h)) {
            return;
        }
        if let Some((key, px)) = std::fs::read(self.cache_path()).ok().and_then(|d| cache::decode(&d, w, h)) {
            self.frame = Some(Frame { w, h, key, px });
            return;
        }
        self.rebuild_frame(w, h, area);
    }

    fn rebuild_frame(&mut self, w: usize, h: usize, area: Rect) {
        if self.text.is_none() {
            self.text = load_font();
        }
        let Some(text) = &mut self.text else { return };
        let frame = render_grid(&self.jni, text, &self.slots, w, h, area, self.dp);
        save(&self.cache_path(), &cache::encode(frame.key, w, h, &frame.px));
        self.frame = Some(frame);
    }

    fn cache_path(&self) -> PathBuf {
        self.files.join("grid.cache")
    }

    // ------------------------------------------------- background freshness check

    /// Re-reads labels, icons and update times off the main thread; if they changed since the
    /// cached grid was drawn, the worker renders a new one.
    fn start_check(&mut self) {
        if self.checking {
            return;
        }
        let (Some((w, h, area)), Some(frame)) = (self.geometry(), &self.frame) else { return };
        self.checking = true;
        let (jni, slots, dp, old_key, tx) = (self.jni.clone(), self.slots.clone(), self.dp, frame.key, self.tx.clone());
        let waker = self.app.create_waker();
        std::thread::spawn(move || {
            let mut refreshed = None;
            if grid_key(&jni, &slots, w, h, dp) != old_key
                && let Some(mut text) = load_font()
            {
                log::info!("grid is stale, redrawing");
                let frame = render_grid(&jni, &mut text, &slots, w, h, area, dp);
                refreshed = Some(Refreshed { slots: slots.serialize(), frame });
            }
            let _ = tx.send(refreshed);
            waker.wake();
        });
    }

    fn take_refresh(&mut self) {
        while let Ok(r) = self.rx.try_recv() {
            self.checking = false;
            // Ignore a result for slots that changed while the worker ran.
            if let Some(r) = r
                && r.slots == self.slots.serialize()
            {
                save(&self.cache_path(), &cache::encode(r.frame.key, r.frame.w, r.frame.h, &r.frame.px));
                self.frame = Some(r.frame);
                if matches!(self.screen, Screen::Grid { .. }) {
                    self.redraw();
                }
            }
        }
    }

    // ------------------------------------------------------------------ input

    fn drain_input(&mut self) {
        let app = self.app.clone();
        let Ok(mut events) = app.input_events_iter() else { return };
        while events.next(|event| match event {
            InputEvent::KeyEvent(k) => self.on_key(k),
            InputEvent::MotionEvent(m) => self.on_motion(m),
            _ => InputStatus::Unhandled,
        }) {}
    }

    fn on_key(&mut self, k: &KeyEvent) -> InputStatus {
        let down = k.action() == KeyAction::Down;
        let first = down && k.repeat_count() == 0;
        let code = k.key_code();
        if matches!(self.screen, Screen::Picker(_)) {
            return self.on_picker_key(code, down);
        }
        let Screen::Grid { focus } = self.screen else { return InputStatus::Unhandled };
        if let Some(i) = slot_for_key(code) {
            if first {
                self.activate(i);
            }
            return InputStatus::Handled;
        }
        match code {
            Keycode::DpadUp | Keycode::DpadDown | Keycode::DpadLeft | Keycode::DpadRight => {
                if first {
                    let next = match focus {
                        None => SLOTS / 2 - 2, // '5'
                        Some(i) => move_focus(i, code),
                    };
                    self.screen = Screen::Grid { focus: Some(next) };
                    self.redraw();
                }
                InputStatus::Handled
            }
            Keycode::DpadCenter | Keycode::Enter | Keycode::NumpadEnter => {
                let i = focus.unwrap_or(SLOTS / 2 - 2);
                if first {
                    self.ok_down = Some(false);
                } else if down && self.ok_down == Some(false) {
                    self.ok_down = Some(true);
                    self.open_picker(i);
                } else if !down {
                    if self.ok_down == Some(false) {
                        if focus.is_none() {
                            self.screen = Screen::Grid { focus: Some(i) };
                            self.redraw();
                        } else {
                            self.activate(i);
                        }
                    }
                    self.ok_down = None;
                }
                InputStatus::Handled
            }
            Keycode::Menu => {
                if first {
                    self.open_picker(focus.unwrap_or(SLOTS / 2 - 2));
                }
                InputStatus::Handled
            }
            Keycode::Back => {
                if !down {
                    self.jni.move_task_to_back();
                }
                InputStatus::Handled
            }
            _ => InputStatus::Unhandled,
        }
    }

    fn on_picker_key(&mut self, code: Keycode, down: bool) -> InputStatus {
        let rows = self.picker_rows();
        let Screen::Picker(p) = &mut self.screen else { return InputStatus::Unhandled };
        match code {
            Keycode::DpadUp | Keycode::DpadDown | Keycode::DpadLeft | Keycode::DpadRight if down => {
                match code {
                    Keycode::DpadUp => p.step(-1, rows),
                    Keycode::DpadDown => p.step(1, rows),
                    Keycode::DpadLeft => p.page(-1, rows),
                    _ => p.page(1, rows),
                }
                self.redraw();
                InputStatus::Handled
            }
            Keycode::DpadCenter | Keycode::Enter | Keycode::NumpadEnter => {
                // `ok_down` is set when a long press on the grid opened this picker; that
                // press's release must not also pick.
                if !down && self.ok_down.take().is_none() {
                    let (slot, sel) = (p.slot, p.selected);
                    self.assign(slot, sel);
                }
                InputStatus::Handled
            }
            Keycode::Back => {
                if !down {
                    let slot = p.slot;
                    self.screen = Screen::Grid { focus: Some(slot) };
                    self.redraw();
                }
                InputStatus::Handled
            }
            _ if slot_for_key(code).is_some() => InputStatus::Handled,
            _ => InputStatus::Unhandled,
        }
    }

    fn on_motion(&mut self, m: &MotionEvent) -> InputStatus {
        let p = m.pointer_at_index(m.pointer_index());
        let (x, y) = (p.x(), p.y());
        let slop = self.dp.pxf(10.0);
        match m.action() {
            MotionAction::Down => {
                let top = if let Screen::Picker(p) = &self.screen { p.top } else { 0 };
                self.touch = Some(Touch { x, y, at: Instant::now(), moved: false, top });
            }
            MotionAction::Move => {
                let row_h = self.dp.px(52.0) as f32;
                let rows = self.picker_rows();
                let Some(t) = &mut self.touch else { return InputStatus::Handled };
                if (y - t.y).abs() > slop || (x - t.x).abs() > slop {
                    t.moved = true;
                }
                if let (true, Screen::Picker(p)) = (t.moved, &mut self.screen) {
                    let max_top = p.items.len().saturating_sub(rows);
                    let top = (t.top as f32 - (y - t.y) / row_h).round().clamp(0.0, max_top as f32) as usize;
                    if top != p.top {
                        p.top = top;
                        p.selected = p.selected.clamp(top, top + rows - 1);
                        self.redraw();
                    }
                }
            }
            MotionAction::Up => {
                let Some(t) = self.touch.take() else { return InputStatus::Handled };
                if !t.moved {
                    self.on_tap(x as i32, y as i32, t.at.elapsed().as_millis() >= LONG_PRESS_MS);
                }
            }
            MotionAction::Cancel => self.touch = None,
            _ => {}
        }
        InputStatus::Handled
    }

    fn on_tap(&mut self, x: i32, y: i32, long: bool) {
        let Some((_, _, area)) = self.geometry() else { return };
        match &mut self.screen {
            Screen::Grid { .. } => {
                if let Some(i) = cell_at(&GridLayout::new(area, self.dp).cells, x, y) {
                    if long { self.open_picker(i) } else { self.activate(i) }
                }
            }
            Screen::Picker(p) => {
                if let Some(i) = PickerLayout::new(area, self.dp).row_at(p, y) {
                    let slot = p.slot;
                    p.selected = i;
                    self.assign(slot, i);
                }
            }
        }
    }

    // ---------------------------------------------------------------- actions

    /// Launches the app in slot `i`, or opens the picker for an empty slot.
    fn activate(&mut self, i: usize) {
        let Some(component) = self.slots.get(i).map(str::to_string) else {
            self.open_picker(i);
            return;
        };
        let t = Instant::now();
        if self.jni.launch(&component) {
            log::info!("launched slot {i} in {} ms", t.elapsed().as_millis());
            return;
        }
        log::warn!("{component} can't be started; clearing slot {i}");
        self.slots.set(i, None);
        self.save_slots_and_rebuild();
        self.redraw();
    }

    fn open_picker(&mut self, slot: usize) {
        if self.text.is_none() {
            self.text = load_font();
        }
        if let (Some((_, _, area)), Some(window), Some(text)) = (self.geometry(), &self.window, &mut self.text) {
            // The app list takes a moment on this phone; say so first.
            if let Ok(mut buf) = window.lock(None) {
                let (w, h, stride) = (buf.width(), buf.height(), buf.stride());
                // SAFETY: as in `redraw`.
                let px = unsafe { std::slice::from_raw_parts_mut(buf.bits().cast::<u32>(), stride * h) };
                ui::draw_message(&mut Canvas::new(px, w, h, stride), area, self.dp, text, "Loading apps…");
            }
        }
        let t = Instant::now();
        let apps = self.jni.launcher_apps(OWN_PACKAGE);
        log::info!("listed {} apps in {} ms", apps.len(), t.elapsed().as_millis());
        let mut picker = Picker::new(slot, apps, self.slots.get(slot));
        picker.scroll_into_view(self.picker_rows());
        self.screen = Screen::Picker(picker);
        self.redraw();
    }

    fn assign(&mut self, slot: usize, item: usize) {
        let Screen::Picker(p) = &self.screen else { return };
        let component = p.items.get(item).and_then(|it| it.component.clone());
        self.slots.set(slot, component);
        self.save_slots_and_rebuild();
        self.screen = Screen::Grid { focus: Some(slot) };
        self.redraw();
    }

    fn save_slots_and_rebuild(&mut self) {
        save(&self.files.join("slots.txt"), self.slots.serialize().as_bytes());
        if let Some((w, h, area)) = self.geometry() {
            self.rebuild_frame(w, h, area);
        } else {
            self.frame = None;
            let _ = std::fs::remove_file(self.cache_path());
        }
    }

    fn picker_rows(&self) -> usize {
        self.geometry().map_or(1, |(_, _, area)| PickerLayout::new(area, self.dp).visible_rows())
    }
}

/// Grid cell for a keypad key, in keypad order.
fn slot_for_key(code: Keycode) -> Option<usize> {
    Some(match code {
        Keycode::Keycode1 => 0,
        Keycode::Keycode2 => 1,
        Keycode::Keycode3 => 2,
        Keycode::Keycode4 => 3,
        Keycode::Keycode5 => 4,
        Keycode::Keycode6 => 5,
        Keycode::Keycode7 => 6,
        Keycode::Keycode8 => 7,
        Keycode::Keycode9 => 8,
        Keycode::Star => 9,
        Keycode::Keycode0 => 10,
        Keycode::Pound => 11,
        _ => return None,
    })
}

fn move_focus(i: usize, code: Keycode) -> usize {
    let (c, r) = (i % COLS, i / COLS);
    let (c, r) = match code {
        Keycode::DpadLeft => (c.saturating_sub(1), r),
        Keycode::DpadRight => ((c + 1).min(COLS - 1), r),
        Keycode::DpadUp => (c, r.saturating_sub(1)),
        _ => (c, (r + 1).min(ROWS - 1)),
    };
    r * COLS + c
}

/// Everything the grid image depends on. Calls into PackageManager once per filled slot.
fn grid_key(jni: &Jni, slots: &Slots, w: usize, h: usize, dp: Dp) -> u64 {
    let mut k = Fnv::default()
        .u64(THEME_VERSION)
        .u64(w as u64)
        .u64(h as u64)
        .u64(dp.0.to_bits() as u64)
        .bytes(slots.serialize().as_bytes());
    for c in slots.0.iter().flatten() {
        k = k.u64(jni.last_update(c).unwrap_or(-1) as u64);
    }
    k.finish()
}

fn render_grid(jni: &Jni, text: &mut Text, slots: &Slots, w: usize, h: usize, area: Rect, dp: Dp) -> Frame {
    let t = Instant::now();
    let layout = GridLayout::new(area, dp);
    let data: Vec<Option<(String, Option<Icon>)>> = slots
        .0
        .iter()
        .map(|s| {
            let c = s.as_deref()?;
            let label = jni.label(c)?;
            Some((label, jni.icon(c, layout.icon)))
        })
        .collect();
    let views: [Option<SlotView>; SLOTS] =
        std::array::from_fn(|i| data[i].as_ref().map(|(label, icon)| SlotView { label, icon: icon.as_ref() }));
    let mut px = vec![0u32; w * h];
    ui::draw_grid(&mut Canvas::new(&mut px, w, h, w), &layout, text, &views);
    let key = grid_key(jni, slots, w, h, dp);
    log::info!("rendered grid in {} ms", t.elapsed().as_millis());
    Frame { w, h, key, px }
}

fn load_font() -> Option<Text> {
    for path in FONTS {
        if let Ok(bytes) = std::fs::read(path) {
            match Text::from_bytes(&bytes) {
                Ok(t) => return Some(t),
                Err(e) => log::warn!("{path}: {e}"),
            }
        }
    }
    log::error!("no usable font");
    None
}

/// Writes through a temporary file and a rename, so a crash never leaves half a file.
fn save(path: &Path, data: &[u8]) {
    let tmp = path.with_extension("tmp");
    let r = std::fs::create_dir_all(path.parent().unwrap_or(Path::new("/")))
        .and_then(|_| std::fs::write(&tmp, data))
        .and_then(|_| std::fs::rename(&tmp, path));
    if let Err(e) = r {
        log::warn!("saving {}: {e}", path.display());
    }
}
