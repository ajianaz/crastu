//! Spike #4: macOS push-to-talk hotkey and text insertion into the focused app.
//!
//! Runs as a windowless agent app (see `bundle.sh`) so that Accessibility and Input
//! Monitoring are granted to the spike itself, as they will be for Crastu.app.
//!
//! Usage (arguments after `open … --args`):
//!   check [--prompt]                       permission status, optionally asking for them
//!   run --key <rightoption|rightcmd|fn|f13> --sink <paste|paste-promise|type>
//!       [--minutes N] [--pass-through]
//!
//! While running: hold the hotkey, release it, and a test string is inserted into the app
//! that was focused when the key went down. Everything is appended to
//! `~/Library/Logs/crastu-spike-input.log`.
//!
//! Unsafe code: raw CoreGraphics, AppKit and Accessibility calls. This crate is not part of
//! the Crastu workspace; the chosen approach gets a safe wrapper in `crastu-os-input`.

use std::{
    cell::{Cell, RefCell},
    env,
    ffi::c_void,
    fs::OpenOptions,
    io::Write as _,
    ptr::{self, NonNull},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, anyhow, bail};
use objc2::{
    AllocAnyThread, DefinedClass, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSPasteboard, NSPasteboardItem, NSPasteboardItemDataProvider,
    NSPasteboardType, NSPasteboardTypeString, NSPasteboardWriting, NSWorkspace,
};
use objc2_application_services::{AXIsProcessTrusted, AXIsProcessTrustedWithOptions};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFMachPort, CFRetained, CFRunLoop, CFString, kCFRunLoopCommonModes,
    kCFRunLoopDefaultMode,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
    CGPreflightListenEventAccess, CGPreflightPostEventAccess, CGRequestListenEventAccess,
    CGRequestPostEventAccess,
};
use objc2_foundation::{NSArray, NSData, NSString};

mod pill;

const LOG_FILE: &str = "Library/Logs/crastu-spike-input.log";
/// Holds shorter than this are treated as taps and ignored.
const MIN_HOLD: Duration = Duration::from_millis(200);

// Virtual key codes (HIToolbox Events.h).
const KC_RIGHT_COMMAND: i64 = 54;
const KC_RIGHT_OPTION: i64 = 61;
const KC_FUNCTION: i64 = 63;
const KC_F13: i64 = 105;
const KC_ANSI_V: u16 = 9;
// Device-specific modifier bits (IOLLEvent.h): tell right from left keys.
const NX_DEVICERCMDKEYMASK: u64 = 0x10;
const NX_DEVICERALTKEYMASK: u64 = 0x40;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

// ---------------------------------------------------------------------------------------
// Logging

thread_local! {
    static START: Instant = Instant::now();
}

fn log(line: impl AsRef<str>) {
    let elapsed = START.with(|s| s.elapsed());
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let text = format!(
        "[{wall} +{:>8.3}s] {}",
        elapsed.as_secs_f64(),
        line.as_ref()
    );
    eprintln!("{text}");
    if let Some(home) = env::var_os("HOME") {
        let path = std::path::Path::new(&home).join(LOG_FILE);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{text}");
        }
    }
}

// ---------------------------------------------------------------------------------------
// Permissions

fn permissions(prompt: bool) {
    // SAFETY: plain status queries.
    let accessibility = unsafe { AXIsProcessTrusted() };
    let listen = CGPreflightListenEventAccess();
    let post = CGPreflightPostEventAccess();
    log(format!(
        "permissions: accessibility={accessibility} input_monitoring(listen)={listen} post_events={post}"
    ));
    if prompt {
        let key = CFString::from_static_str("AXTrustedCheckOptionPrompt");
        let value = CFBoolean::new(true);
        let options = CFDictionary::<CFString, CFBoolean>::from_slices(&[&key], &[value]);
        // SAFETY: the dictionary holds the documented prompt option.
        let ax = unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) };
        let listen = CGRequestListenEventAccess();
        let post = CGRequestPostEventAccess();
        log(format!(
            "after prompt: accessibility={ax} input_monitoring(listen)={listen} post_events={post}"
        ));
    }
}

// ---------------------------------------------------------------------------------------
// Hotkey source

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    RightOption,
    RightCommand,
    Function,
    F13,
}

impl Key {
    fn keycode(self) -> i64 {
        match self {
            Self::RightOption => KC_RIGHT_OPTION,
            Self::RightCommand => KC_RIGHT_COMMAND,
            Self::Function => KC_FUNCTION,
            Self::F13 => KC_F13,
        }
    }

    fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "rightoption" => Self::RightOption,
            "rightcmd" => Self::RightCommand,
            "fn" => Self::Function,
            "f13" => Self::F13,
            other => bail!("unknown key {other}"),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Edge {
    Down,
    Up,
}

/// Turns raw key events into press and release edges for one configured key.
trait HotkeySource {
    fn edge(&mut self, ty: CGEventType, keycode: i64, flags: u64, autorepeat: bool)
    -> Option<Edge>;
}

struct TapHotkey {
    key: Key,
    held: bool,
}

impl HotkeySource for TapHotkey {
    fn edge(
        &mut self,
        ty: CGEventType,
        keycode: i64,
        flags: u64,
        autorepeat: bool,
    ) -> Option<Edge> {
        let now_down = match self.key {
            Key::F13 => {
                if keycode != KC_F13 || autorepeat {
                    return None;
                }
                if ty == CGEventType::KeyDown {
                    true
                } else if ty == CGEventType::KeyUp {
                    false
                } else {
                    return None;
                }
            }
            modifier => {
                if ty != CGEventType::FlagsChanged {
                    return None;
                }
                let (code, down) = match modifier {
                    Key::RightOption => (KC_RIGHT_OPTION, flags & NX_DEVICERALTKEYMASK != 0),
                    Key::RightCommand => (KC_RIGHT_COMMAND, flags & NX_DEVICERCMDKEYMASK != 0),
                    Key::Function => (KC_FUNCTION, flags & CGEventFlags::MaskSecondaryFn.0 != 0),
                    Key::F13 => unreachable!(),
                };
                if keycode != code {
                    return None;
                }
                down
            }
        };
        match (self.held, now_down) {
            (false, true) => {
                self.held = true;
                Some(Edge::Down)
            }
            (true, false) => {
                self.held = false;
                Some(Edge::Up)
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Target app

#[derive(Clone, Debug)]
struct Target {
    pid: i32,
    bundle: String,
    name: String,
}

fn frontmost() -> Option<Target> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(Target {
        pid: app.processIdentifier(),
        bundle: app
            .bundleIdentifier()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        name: app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_default(),
    })
}

fn reactivate(target: &Target) -> bool {
    match objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(target.pid) {
        Some(app) => app.activateWithOptions(NSApplicationActivationOptions(0)),
        None => false,
    }
}

// ---------------------------------------------------------------------------------------
// Clipboard snapshot

struct Snapshot {
    items: Vec<Vec<(Retained<NSPasteboardType>, Retained<NSData>)>>,
}

impl Snapshot {
    fn take(pb: &NSPasteboard) -> Self {
        let mut items = Vec::new();
        if let Some(list) = pb.pasteboardItems() {
            for item in list.iter() {
                let mut entries = Vec::new();
                for ty in item.types().iter() {
                    if let Some(data) = item.dataForType(&ty) {
                        entries.push((ty.clone(), data));
                    }
                }
                items.push(entries);
            }
        }
        Self { items }
    }

    fn summary(&self) -> String {
        let types: usize = self.items.iter().map(Vec::len).sum();
        let bytes: usize = self.items.iter().flatten().map(|(_, d)| d.length()).sum();
        format!(
            "{} item(s), {types} type(s), {bytes} bytes",
            self.items.len()
        )
    }

    fn restore(&self, pb: &NSPasteboard) -> bool {
        pb.clearContents();
        if self.items.is_empty() {
            return true;
        }
        let objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = self
            .items
            .iter()
            .map(|entries| {
                let item = NSPasteboardItem::new();
                for (ty, data) in entries {
                    item.setData_forType(data, ty);
                }
                ProtocolObject::from_retained(item)
            })
            .collect();
        pb.writeObjects(&NSArray::from_retained_slice(&objects))
    }

    fn matches(&self, other: &Snapshot) -> bool {
        self.items.len() == other.items.len()
            && self.items.iter().zip(&other.items).all(|(a, b)| {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|((ta, da), (tb, db))| ta.isEqualToString(tb) && da.isEqual(Some(db)))
            })
    }
}

// ---------------------------------------------------------------------------------------
// Promised pasteboard data

thread_local! {
    static PROVIDED: Cell<u32> = const { Cell::new(0) };
    static FIRST_READ: Cell<Option<Instant>> = const { Cell::new(None) };
}

struct ProviderIvars {
    text: Retained<NSString>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the ivars are immutable.
    #[unsafe(super(NSObject))]
    #[name = "CrastuSpikePasteProvider"]
    #[ivars = ProviderIvars]
    struct Provider;

    unsafe impl NSObjectProtocol for Provider {}

    unsafe impl NSPasteboardItemDataProvider for Provider {
        #[unsafe(method(pasteboard:item:provideDataForType:))]
        fn provide(
            &self,
            _pasteboard: Option<&NSPasteboard>,
            item: &NSPasteboardItem,
            ty: &NSPasteboardType,
        ) {
            item.setString_forType(&self.ivars().text, ty);
            PROVIDED.with(|c| c.set(c.get() + 1));
            FIRST_READ.with(|f| {
                if f.get().is_none() {
                    f.set(Some(Instant::now()));
                }
            });
        }
    }
);

impl Provider {
    fn new(text: &str) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ProviderIvars {
            text: NSString::from_str(text),
        });
        // SAFETY: NSObject's init on a freshly allocated instance.
        unsafe { msg_send![super(this), init] }
    }
}

// ---------------------------------------------------------------------------------------
// Text sinks

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SinkKind {
    Paste,
    PastePromise,
    Type,
}

impl SinkKind {
    fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "paste" => Self::Paste,
            "paste-promise" => Self::PastePromise,
            "type" => Self::Type,
            other => bail!("unknown sink {other}"),
        })
    }
}

/// Inserts text into the focused app.
trait TextSink {
    fn insert(&mut self, text: &str) -> Result<String>;
}

/// Spin the main run loop so pasteboard requests and the event tap keep being served.
fn spin(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        // SAFETY: the default mode constant is a static CF string.
        let mode = unsafe { kCFRunLoopDefaultMode };
        CFRunLoop::run_in_mode(mode, left.as_secs_f64().min(0.01), true);
    }
}

fn spin_until(timeout: Duration, done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        spin(Duration::from_millis(5));
    }
    done()
}

fn post_cmd_v() -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
    for down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.as_deref(), KC_ANSI_V, down)
            .ok_or_else(|| anyhow!("create Cmd+V event"))?;
        CGEvent::set_flags(Some(&event), CGEventFlags::MaskCommand);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
    Ok(())
}

/// Splits text into UTF-16 chunks of at most `max` units without splitting a character.
fn utf16_chunks(text: &str, max: usize) -> Vec<Vec<u16>> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    for ch in text.chars() {
        let mut buf = [0u16; 2];
        let units = ch.encode_utf16(&mut buf);
        if current.len() + units.len() > max {
            chunks.push(std::mem::take(&mut current));
        }
        current.extend_from_slice(units);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn type_text(text: &str) -> Result<usize> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
    let chunks = utf16_chunks(text, 20);
    for chunk in &chunks {
        for down in [true, false] {
            let event = CGEvent::new_keyboard_event(source.as_deref(), 0, down)
                .ok_or_else(|| anyhow!("create keyboard event"))?;
            // SAFETY: the pointer and length describe the live chunk.
            unsafe {
                CGEvent::keyboard_set_unicode_string(Some(&event), chunk.len() as _, chunk.as_ptr())
            };
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        }
        spin(Duration::from_millis(3));
    }
    Ok(chunks.len())
}

struct PasteSink {
    promise: bool,
    restore_after: Duration,
}

impl TextSink for PasteSink {
    fn insert(&mut self, text: &str) -> Result<String> {
        let pb = NSPasteboard::generalPasteboard();
        let saved = Snapshot::take(&pb);
        pb.clearContents();
        // SAFETY: AppKit constant.
        let string_type = unsafe { NSPasteboardTypeString };
        let _provider_keepalive = if self.promise {
            PROVIDED.with(|c| c.set(0));
            FIRST_READ.with(|f| f.set(None));
            let provider = Provider::new(text);
            let item = NSPasteboardItem::new();
            let types = NSArray::from_slice(&[string_type]);
            item.setDataProvider_forTypes(ProtocolObject::from_ref(&*provider), &types);
            let objects = [ProtocolObject::<dyn NSPasteboardWriting>::from_retained(
                item,
            )];
            if !pb.writeObjects(&NSArray::from_retained_slice(&objects)) {
                bail!("write promised item");
            }
            Some(provider)
        } else {
            pb.setString_forType(&NSString::from_str(text), string_type);
            None
        };
        let change = pb.changeCount();
        let sent = Instant::now();
        post_cmd_v()?;

        let mut report;
        if self.promise {
            let read = spin_until(Duration::from_millis(1000), || PROVIDED.with(Cell::get) > 0);
            if read {
                let after = FIRST_READ
                    .with(Cell::get)
                    .map(|t| t.duration_since(sent).as_secs_f64() * 1000.0)
                    .unwrap_or(-1.0);
                // Let the target finish reading other representations, if any.
                spin(Duration::from_millis(50));
                report = format!(
                    "promise read after {after:.1} ms ({} request(s))",
                    PROVIDED.with(Cell::get)
                );
            } else {
                report = "promise NOT read within 1000 ms -> typing fallback".to_string();
                // Never overwrite a clipboard someone else changed in the meantime.
                let restored = pb.changeCount() == change && saved.restore(&pb);
                let chunks = type_text(text)?;
                report.push_str(&format!(
                    ", typed {chunks} chunk(s), clipboard restore wrote={restored}"
                ));
                return Ok(report);
            }
        } else {
            spin(self.restore_after);
            report = format!(
                "pasted, restored after fixed {} ms",
                self.restore_after.as_millis()
            );
        }

        if pb.changeCount() != change {
            // The user or another app copied something while we were pasting: keep it.
            report.push_str(&format!(
                "; clipboard before: {}; changed by someone else meanwhile, NOT restored",
                saved.summary()
            ));
            return Ok(report);
        }
        let wrote = saved.restore(&pb);
        let check = Snapshot::take(&pb);
        report.push_str(&format!(
            "; clipboard before: {}; restore wrote={wrote} identical={}",
            saved.summary(),
            saved.matches(&check)
        ));
        Ok(report)
    }
}

struct TypeSink;

impl TextSink for TypeSink {
    fn insert(&mut self, text: &str) -> Result<String> {
        let chunks = type_text(text)?;
        Ok(format!("typed {chunks} chunk(s)"))
    }
}

// ---------------------------------------------------------------------------------------
// Event tap

#[derive(Default)]
struct Hold {
    since: Option<Instant>,
    target: Option<Target>,
    secure_input: bool,
    chord: bool,
}

struct State {
    hotkey: TapHotkey,
    swallow: bool,
    hold: Hold,
    pending: Option<(Duration, Option<Target>, bool)>,
    tap: Option<CFRetained<CFMachPort>>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Takes the event as a raw pointer: for the tap-disabled notifications CoreGraphics may pass
/// NULL, which must never be held in a `NonNull`.
unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    ty: CGEventType,
    event: *mut CGEvent,
    _info: *mut c_void,
) -> *mut CGEvent {
    STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return event;
        };
        if ty == CGEventType::TapDisabledByTimeout || ty == CGEventType::TapDisabledByUserInput {
            log(format!("tap disabled by the system ({ty:?}); re-enabling"));
            // A key-up may have been missed while the tap was off.
            state.hotkey.held = false;
            state.hold = Hold::default();
            if let Some(tap) = &state.tap {
                CGEvent::tap_enable(tap, true);
            }
            return event;
        }
        // SAFETY: for keyboard events CoreGraphics passes a live event for the duration of
        // the callback; NULL is checked first.
        let Some(ev) = (unsafe { event.as_ref() }) else {
            return event;
        };
        let pass = event;
        let keycode = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode);
        let autorepeat =
            CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventAutorepeat) != 0;
        let flags = CGEvent::flags(Some(ev)).0;

        match state.hotkey.edge(ty, keycode, flags, autorepeat) {
            Some(Edge::Down) => {
                // SAFETY: plain status query.
                let secure = unsafe { IsSecureEventInputEnabled() } != 0;
                state.hold = Hold {
                    since: Some(Instant::now()),
                    target: frontmost(),
                    secure_input: secure,
                    chord: false,
                };
                log(format!(
                    "key down; target {:?}; secure input {secure}",
                    state.hold.target
                ));
                if state.swallow {
                    return ptr::null_mut();
                }
            }
            Some(Edge::Up) => {
                let held = state.hold.since.map(|s| s.elapsed()).unwrap_or_default();
                let hold = std::mem::take(&mut state.hold);
                if hold.chord {
                    log(format!(
                        "key up after {:.0} ms: used as a modifier in a shortcut, ignored",
                        held.as_secs_f64() * 1000.0
                    ));
                } else if held < MIN_HOLD {
                    log(format!(
                        "key up after {:.0} ms: tap, ignored",
                        held.as_secs_f64() * 1000.0
                    ));
                } else {
                    state.pending = Some((held, hold.target, hold.secure_input));
                }
                if state.swallow {
                    return ptr::null_mut();
                }
            }
            None => {
                if state.hold.since.is_some() {
                    if keycode == state.hotkey.key.keycode() {
                        // The hotkey's own repeat while held: not a shortcut.
                        if state.swallow {
                            return ptr::null_mut();
                        }
                    } else if ty == CGEventType::KeyDown {
                        state.hold.chord = true;
                    }
                }
            }
        }
        pass
    })
}

fn run(key: Key, sink_kind: SinkKind, minutes: u64, swallow: bool) -> Result<()> {
    permissions(false);
    let mask: u64 = (1 << CGEventType::KeyDown.0)
        | (1 << CGEventType::KeyUp.0)
        | (1 << CGEventType::FlagsChanged.0);
    let options = if swallow {
        CGEventTapOptions::Default
    } else {
        CGEventTapOptions::ListenOnly
    };
    // SAFETY: the callback has the documented signature and uses thread-local state only.
    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            options,
            mask,
            // SAFETY: `*mut CGEvent` and `NonNull<CGEvent>` have the same ABI; the callback
            // only differs in accepting NULL, which CoreGraphics may pass for notifications.
            Some(std::mem::transmute::<
                unsafe extern "C-unwind" fn(
                    CGEventTapProxy,
                    CGEventType,
                    *mut CGEvent,
                    *mut c_void,
                ) -> *mut CGEvent,
                unsafe extern "C-unwind" fn(
                    CGEventTapProxy,
                    CGEventType,
                    NonNull<CGEvent>,
                    *mut c_void,
                ) -> *mut CGEvent,
            >(tap_callback)),
            ptr::null_mut(),
        )
    }
    .ok_or_else(|| {
        anyhow!(
            "CGEventTapCreate failed: grant Input Monitoring (and Accessibility for an active tap)"
        )
    })?;
    let source = CFMachPort::new_run_loop_source(None, Some(&tap), 0)
        .ok_or_else(|| anyhow!("run loop source"))?;
    let main = CFRunLoop::main().ok_or_else(|| anyhow!("main run loop"))?;
    // SAFETY: CF constant.
    main.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&tap, true);

    STATE.with(|cell| {
        *cell.borrow_mut() = Some(State {
            hotkey: TapHotkey { key, held: false },
            swallow,
            hold: Hold::default(),
            pending: None,
            tap: Some(tap),
        });
    });
    log(format!(
        "running: key {key:?}, sink {sink_kind:?}, {} tap, for {minutes} min",
        if swallow {
            "active (hotkey swallowed)"
        } else {
            "listen-only"
        }
    ));

    let mut sink: Box<dyn TextSink> = match sink_kind {
        SinkKind::Paste => Box::new(PasteSink {
            promise: false,
            restore_after: Duration::from_millis(250),
        }),
        SinkKind::PastePromise => Box::new(PasteSink {
            promise: true,
            restore_after: Duration::from_millis(250),
        }),
        SinkKind::Type => Box::new(TypeSink),
    };

    let end = Instant::now() + Duration::from_secs(minutes * 60);
    let mut count = 0u32;
    while Instant::now() < end {
        // SAFETY: CF constant.
        CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, 0.25, true);
        let job = STATE.with(|cell| cell.borrow_mut().as_mut().and_then(|s| s.pending.take()));
        let Some((held, target, secure)) = job else {
            continue;
        };
        count += 1;
        let text = format!(
            "Crastu spike {count} ({sink_kind:?}): naïve café, Grüße, 日本語, emoji 🙂👍🏽, done."
        );
        let now_front = frontmost();
        let mut note = String::new();
        if let (Some(t), Some(n)) = (&target, &now_front)
            && t.pid != n.pid
        {
            let ok = reactivate(t);
            spin(Duration::from_millis(150));
            note = format!(" (focus moved to {}; reactivated {}: {ok})", n.name, t.name);
        }
        let started = Instant::now();
        let result = sink.insert(&text);
        let took = started.elapsed().as_secs_f64() * 1000.0;
        log(format!(
            "#{count} held {:.0} ms -> {} [{}]{note}{}: {} in {took:.1} ms",
            held.as_secs_f64() * 1000.0,
            target.as_ref().map(|t| t.name.as_str()).unwrap_or("?"),
            target.as_ref().map(|t| t.bundle.as_str()).unwrap_or("?"),
            if secure { " SECURE INPUT ON" } else { "" },
            match &result {
                Ok(r) => r.clone(),
                Err(e) => format!("ERROR {e:#}"),
            }
        ));
    }
    log("time is up; exiting");
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Self-test of the parts that need no permission

fn verdict(name: &str, ok: bool, detail: String) -> u32 {
    log(format!(
        "selftest {name}: {} {detail}",
        if ok { "PASS" } else { "FAIL" }
    ));
    u32::from(!ok)
}

fn make_item(parts: &[(&str, Vec<u8>)]) -> Retained<NSPasteboardItem> {
    let item = NSPasteboardItem::new();
    for (ty, bytes) in parts {
        item.setData_forType(&NSData::with_bytes(bytes), &NSString::from_str(ty));
    }
    item
}

fn write_items(pb: &NSPasteboard, items: Vec<Retained<NSPasteboardItem>>) -> bool {
    let objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = items
        .into_iter()
        .map(ProtocolObject::from_retained)
        .collect();
    pb.clearContents();
    pb.writeObjects(&NSArray::from_retained_slice(&objects))
}

/// Exercises the clipboard snapshot, restore, change count and promised data without
/// Accessibility or Input Monitoring. The user's own clipboard is saved first and put back
/// at the end.
fn selftest() -> Result<()> {
    let pb = NSPasteboard::generalPasteboard();
    let original = Snapshot::take(&pb);
    log(format!(
        "selftest: your clipboard holds {}; it is restored at the end",
        original.summary()
    ));
    // SAFETY: AppKit constant.
    let string_type = unsafe { NSPasteboardTypeString };
    let mut failures = 0;

    // 1. A multi-type, multi-item clipboard survives snapshot, overwrite and restore.
    let png: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(31) ^ (i >> 3)) as u8)
        .collect();
    let rtf = br"{\rtf1\ansi Crastu \b spike\b0 }".to_vec();
    let first = make_item(&[
        ("public.utf8-plain-text", "Halo café 🙂".as_bytes().to_vec()),
        ("public.rtf", rtf),
        ("public.png", png),
        ("dev.crastu.spike.blob", vec![0, 1, 2, 255, 254]),
    ]);
    let second = make_item(&[("public.utf8-plain-text", b"second item".to_vec())]);
    let wrote = write_items(&pb, vec![first, second]);
    let before = Snapshot::take(&pb);
    failures += verdict(
        "write multi-type",
        wrote && before.items.len() == 2,
        format!(
            "-> {}; types per item: {:?}",
            before.summary(),
            before
                .items
                .iter()
                .map(|entries| entries
                    .iter()
                    .map(|(t, _)| t.to_string())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>()
        ),
    );
    pb.clearContents();
    pb.setString_forType(&NSString::from_str("clobber"), string_type);
    // Negative control: the comparison must notice that the clipboard changed.
    let clobbered = Snapshot::take(&pb);
    failures += verdict(
        "comparison detects a difference",
        !before.matches(&clobbered),
        format!("{} vs {}", before.summary(), clobbered.summary()),
    );
    let restored = before.restore(&pb);
    let after = Snapshot::take(&pb);
    failures += verdict(
        "restore multi-type",
        restored && before.matches(&after),
        format!("before {} / after {}", before.summary(), after.summary()),
    );

    // 2. The change count moves on every write, so a restore can be skipped when someone
    //    else wrote in between.
    let c1 = pb.changeCount();
    pb.clearContents();
    let c2 = pb.changeCount();
    failures += verdict(
        "change count moves on write",
        c2 != c1,
        format!("{c1} -> {c2}"),
    );

    // 3. Promised data is produced when it is first read, not when it is written.
    PROVIDED.with(|c| c.set(0));
    let provider = Provider::new("promised text ✓");
    let item = NSPasteboardItem::new();
    item.setDataProvider_forTypes(
        ProtocolObject::from_ref(&*provider),
        &NSArray::from_slice(&[string_type]),
    );
    let wrote = write_items(&pb, vec![item]);
    let lazy = PROVIDED.with(Cell::get);
    let text = pb.stringForType(string_type).map(|s| s.to_string());
    let calls = PROVIDED.with(Cell::get);
    failures += verdict(
        "promised data",
        wrote && text.as_deref() == Some("promised text ✓") && calls > 0,
        format!("provider calls at write {lazy}, after first read {calls}, text {text:?}"),
    );

    // 4. Put the user's clipboard back.
    let back = original.restore(&pb);
    let check = Snapshot::take(&pb);
    failures += verdict(
        "your clipboard restored",
        back && original.matches(&check),
        check.summary(),
    );

    log(format!("selftest done: {failures} failure(s)"));
    if failures > 0 {
        bail!("{failures} self-test check(s) failed");
    }
    Ok(())
}

fn main() -> Result<()> {
    START.with(|_| ());
    let args: Vec<String> = env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    match args.first().map(String::as_str) {
        Some("check") => {
            permissions(flag("--prompt"));
            Ok(())
        }
        Some("selftest") => selftest(),
        Some("pill") => pill::run(&args[1..]),
        Some("run") => {
            let key = Key::parse(&value("--key").unwrap_or_else(|| "rightoption".into()))?;
            let sink = SinkKind::parse(&value("--sink").unwrap_or_else(|| "paste-promise".into()))?;
            let minutes = value("--minutes")
                .map(|m| m.parse())
                .transpose()?
                .unwrap_or(15);
            let result = run(key, sink, minutes, !flag("--pass-through"));
            if let Err(e) = &result {
                log(format!("run failed: {e:#}"));
            }
            result
        }
        _ => {
            log(
                "usage: check [--prompt] | selftest | pill [--seconds N] [--delay N] [--busy] [--activating] | run --key <rightoption|rightcmd|fn|f13> --sink <paste|paste-promise|type> [--minutes N] [--pass-through]",
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_never_split_a_character() {
        let text = "ab🙂👍🏽日本語".repeat(5);
        for chunk in utf16_chunks(&text, 4) {
            assert!(chunk.len() <= 4);
            assert!(String::from_utf16(&chunk).is_ok(), "split surrogate pair");
        }
        let joined: Vec<u16> = utf16_chunks(&text, 4).concat();
        assert_eq!(String::from_utf16(&joined).unwrap(), text);
    }

    #[test]
    fn right_option_edges_ignore_left_option() {
        let mut hk = TapHotkey {
            key: Key::RightOption,
            held: false,
        };
        let alt = CGEventFlags::MaskAlternate.0;
        // Left option (keycode 58) never triggers.
        assert_eq!(
            hk.edge(CGEventType::FlagsChanged, 58, alt | 0x20, false),
            None
        );
        assert_eq!(
            hk.edge(
                CGEventType::FlagsChanged,
                KC_RIGHT_OPTION,
                alt | NX_DEVICERALTKEYMASK,
                false
            ),
            Some(Edge::Down)
        );
        assert_eq!(
            hk.edge(CGEventType::FlagsChanged, KC_RIGHT_OPTION, 0, false),
            Some(Edge::Up)
        );
    }

    #[test]
    fn f13_ignores_autorepeat() {
        let mut hk = TapHotkey {
            key: Key::F13,
            held: false,
        };
        assert_eq!(
            hk.edge(CGEventType::KeyDown, KC_F13, 0, false),
            Some(Edge::Down)
        );
        assert_eq!(hk.edge(CGEventType::KeyDown, KC_F13, 0, true), None);
        assert_eq!(
            hk.edge(CGEventType::KeyUp, KC_F13, 0, false),
            Some(Edge::Up)
        );
    }
}
