use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xfixes::SelectionEventMask;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, Property,
    SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME, NONE};

use super::{KindGate, MonitorEvent};
use crate::clipboard::error::ClipboardError;
use crate::clipboard::model::{Capture, Payload, PlatformCaps};
use crate::clipboard::util::{
    decode_png, encode_png, kde_secret, parse_gnome_copied, parse_uri_list, uri_list,
};

enum Command {
    Read(Sender<Result<Capture, ClipboardError>>),
    Write(Owned, Sender<Result<(), ClipboardError>>),
}

enum Owned {
    None,
    Text(Vec<u8>),
    Png(Vec<u8>),
    Files { uri_list: Vec<u8>, text: Vec<u8> },
}

struct Atoms {
    clipboard: Atom,
    targets: Atom,
    utf8: Atom,
    png: Atom,
    uri_list: Atom,
    gnome: Atom,
    secret: Atom,
    incr: Atom,
    property: Atom,
    plain: Atom,
    net_wm_pid: Atom,
}

enum Stage {
    Targets,
    Secret,
    Body(Atom),
    Incremental { target: Atom, data: Vec<u8> },
}

struct Pending {
    sink: Option<Sender<Result<Capture, ClipboardError>>>,
    stage: Stage,
    targets: Vec<Atom>,
    owner: Window,
    deadline: Instant,
}

pub struct X11Backend {
    commands: Sender<Command>,
    recording: Arc<AtomicBool>,
    kinds: Arc<KindGate>,
    _thread: JoinHandle<()>,
}

impl X11Backend {
    pub fn start(events: Sender<MonitorEvent>, recording: Arc<AtomicBool>) -> Result<Self, String> {
        let (commands, receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::channel();
        let flag = Arc::clone(&recording);
        let kinds = Arc::new(KindGate::new());
        let kinds_thread = Arc::clone(&kinds);
        let thread = thread::Builder::new()
            .name("pixo-clipboard".into())
            .spawn(move || run(receiver, events, flag, kinds_thread, ready_sender))
            .map_err(|_| "x11".to_string())?;
        match ready_receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(Self {
                commands,
                recording,
                kinds,
                _thread: thread,
            }),
            _ => Err("x11".into()),
        }
    }

    pub fn capabilities(&self) -> PlatformCaps {
        PlatformCaps {
            platform: "linux-x11",
            can_monitor: true,
            can_read_source_app: true,
            can_global_shortcut: true,
            reason: None,
        }
    }

    pub fn set_recording(&self, recording: bool) {
        self.recording.store(recording, Ordering::SeqCst);
    }

    pub fn set_kinds(&self, text: bool, image: bool, files: bool) {
        self.kinds.set(text, image, files);
    }

    pub fn read_now(&self) -> Result<Capture, ClipboardError> {
        self.request(|reply| Command::Read(reply))
    }

    pub fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        self.write(Owned::Text(text.as_bytes().to_vec()))
    }

    pub fn write_image(&self, width: u32, height: u32, rgba: &[u8]) -> Result<(), ClipboardError> {
        let png = encode_png(width, height, rgba).map_err(|_| ClipboardError::new("storage"))?;
        self.write(Owned::Png(png))
    }

    pub fn write_files(&self, paths: &[String]) -> Result<(), ClipboardError> {
        self.write(Owned::Files {
            uri_list: uri_list(paths).into_bytes(),
            text: paths.join("\n").into_bytes(),
        })
    }

    fn write(&self, owned: Owned) -> Result<(), ClipboardError> {
        self.request(|reply| Command::Write(owned, reply))
    }

    fn request<T, F>(&self, command: F) -> Result<T, ClipboardError>
    where
        F: FnOnce(Sender<Result<T, ClipboardError>>) -> Command,
    {
        let (sender, receiver) = mpsc::channel();
        self.commands
            .send(command(sender))
            .map_err(|_| ClipboardError::new("unsupported"))?;
        receiver
            .recv()
            .unwrap_or_else(|_| Err(ClipboardError::new("unsupported")))
    }
}

fn run(
    commands: Receiver<Command>,
    events: Sender<MonitorEvent>,
    recording: Arc<AtomicBool>,
    kinds: Arc<KindGate>,
    ready: Sender<Result<(), String>>,
) {
    let mut context = match Context::connect(kinds) {
        Ok(context) => context,
        Err(error) => {
            eprintln!("clipboard x11: {error}");
            let _ = ready.send(Err("x11".into()));
            while let Ok(command) = commands.recv() {
                fail_command(command);
            }
            return;
        }
    };
    let _ = ready.send(Ok(()));
    loop {
        loop {
            match commands.try_recv() {
                Ok(command) => context.command(command),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
        if context
            .pending
            .as_ref()
            .is_some_and(|pending| Instant::now() > pending.deadline)
        {
            context.finish_pending(&events, Err(ClipboardError::new("busy")));
        }
        match context.conn.poll_for_event() {
            Ok(Some(event)) => context.event(event, &events, &recording),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                eprintln!("clipboard x11 event: {error}");
                break;
            }
        }
    }
}

fn fail_command(command: Command) {
    match command {
        Command::Read(reply) => {
            let _ = reply.send(Err(ClipboardError::new("unsupported")));
        }
        Command::Write(_, reply) => {
            let _ = reply.send(Err(ClipboardError::new("unsupported")));
        }
    }
}

struct Context {
    conn: RustConnection,
    window: Window,
    atoms: Atoms,
    owned: Owned,
    pending: Option<Pending>,
    kinds: Arc<KindGate>,
}

impl Context {
    fn connect(kinds: Arc<KindGate>) -> Result<Self, String> {
        let (conn, screen_number) =
            RustConnection::connect(None).map_err(|error| error.to_string())?;
        let screen = &conn.setup().roots[screen_number];
        let window = conn.generate_id().map_err(|error| error.to_string())?;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(|error| error.to_string())?
        .check()
        .map_err(|error| error.to_string())?;
        conn.xfixes_query_version(5, 0)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        let atoms = Atoms {
            clipboard: intern(&conn, b"CLIPBOARD")?,
            targets: intern(&conn, b"TARGETS")?,
            utf8: intern(&conn, b"UTF8_STRING")?,
            png: intern(&conn, b"image/png")?,
            uri_list: intern(&conn, b"text/uri-list")?,
            gnome: intern(&conn, b"x-special/gnome-copied-files")?,
            secret: intern(&conn, b"x-kde-passwordManagerHint")?,
            incr: intern(&conn, b"INCR")?,
            property: intern(&conn, b"PIXO_CLIP")?,
            plain: intern(&conn, b"text/plain")?,
            net_wm_pid: intern(&conn, b"_NET_WM_PID")?,
        };
        conn.xfixes_select_selection_input(
            screen.root,
            atoms.clipboard,
            SelectionEventMask::SET_SELECTION_OWNER,
        )
        .map_err(|error| error.to_string())?
        .check()
        .map_err(|error| error.to_string())?;
        conn.flush().map_err(|error| error.to_string())?;
        Ok(Self {
            conn,
            window,
            atoms,
            owned: Owned::None,
            pending: None,
            kinds,
        })
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Read(reply) => self.begin_read(Some(reply), 0),
            Command::Write(owned, reply) => {
                self.owned = owned;
                let result = self
                    .conn
                    .set_selection_owner(self.window, self.atoms.clipboard, CURRENT_TIME)
                    .and_then(|cookie| cookie.check())
                    .map_err(|_| ClipboardError::new("busy"));
                let _ = self.conn.flush();
                let _ = reply.send(result);
            }
        }
    }

    fn begin_read(&mut self, sink: Option<Sender<Result<Capture, ClipboardError>>>, owner: Window) {
        if self.pending.is_some() && sink.is_none() {
            return;
        }
        let result = self
            .conn
            .convert_selection(
                self.window,
                self.atoms.clipboard,
                self.atoms.targets,
                self.atoms.property,
                CURRENT_TIME,
            )
            .and_then(|cookie| cookie.check());
        if result.is_err() {
            if let Some(sink) = sink {
                let _ = sink.send(Err(ClipboardError::new("busy")));
            }
            return;
        }
        let _ = self.conn.flush();
        self.pending = Some(Pending {
            sink,
            stage: Stage::Targets,
            targets: Vec::new(),
            owner,
            deadline: Instant::now() + Duration::from_millis(800),
        });
    }

    fn event(&mut self, event: Event, events: &Sender<MonitorEvent>, recording: &AtomicBool) {
        match event {
            Event::XfixesSelectionNotify(notify) if notify.selection == self.atoms.clipboard => {
                if notify.owner == self.window || notify.owner == 0 {
                    return;
                }
                if recording.load(Ordering::SeqCst) {
                    self.begin_read(None, notify.owner);
                }
            }
            Event::SelectionNotify(notify) if notify.property == self.atoms.property => {
                self.selection_notify(events);
            }
            Event::PropertyNotify(notify)
                if notify.atom == self.atoms.property && notify.state == Property::NEW_VALUE =>
            {
                self.incremental_chunk(events);
            }
            Event::SelectionRequest(request) if request.selection == self.atoms.clipboard => {
                self.selection_request(request);
            }
            Event::SelectionClear(clear) if clear.selection == self.atoms.clipboard => {
                self.owned = Owned::None;
            }
            _ => {}
        }
    }

    fn selection_notify(&mut self, events: &Sender<MonitorEvent>) {
        let Some(reply) = self.property() else {
            self.finish_pending(events, Err(ClipboardError::new("busy")));
            return;
        };
        if reply.bytes_after > 0 && reply.type_ != self.atoms.incr {
            self.finish_pending(events, Err(ClipboardError::new("too_large")));
            return;
        }
        if reply.type_ == self.atoms.incr {
            let target = match self.pending.as_ref().map(|pending| &pending.stage) {
                Some(Stage::Body(target)) => *target,
                _ => reply.type_,
            };
            if let Some(pending) = &mut self.pending {
                pending.stage = Stage::Incremental {
                    target,
                    data: Vec::new(),
                };
                pending.deadline = Instant::now() + Duration::from_secs(2);
            }
            return;
        }
        self.consume_bytes(events, reply.value);
    }

    fn incremental_chunk(&mut self, events: &Sender<MonitorEvent>) {
        let Some(reply) = self.property() else {
            return;
        };
        if reply.value.is_empty() {
            let (target, data) = match &self.pending {
                Some(Pending {
                    stage: Stage::Incremental { target, data },
                    ..
                }) => (*target, data.clone()),
                _ => return,
            };
            self.consume_bytes(events, data);
            return;
        }
        if let Some(Pending {
            stage: Stage::Incremental { data, .. },
            deadline,
            ..
        }) = &mut self.pending
        {
            data.extend(reply.value);
            *deadline = Instant::now() + Duration::from_secs(2);
        }
    }

    fn consume_bytes(&mut self, events: &Sender<MonitorEvent>, value: Vec<u8>) {
        let secret_atom = self.atoms.secret;
        enum Step {
            Convert(Atom),
            Body {
                targets: Vec<Atom>,
                owner: Window,
            },
            Ready {
                target: Atom,
                owner: Window,
                value: Vec<u8>,
            },
            Secret {
                owner: Window,
            },
        }
        let step = {
            let Some(pending) = self.pending.as_mut() else {
                return;
            };
            match &pending.stage {
                Stage::Targets => {
                    pending.targets = atoms_from(&value);
                    let targets = pending.targets.clone();
                    let owner = pending.owner;
                    if targets.contains(&secret_atom) {
                        pending.stage = Stage::Secret;
                        Step::Convert(secret_atom)
                    } else {
                        Step::Body { targets, owner }
                    }
                }
                Stage::Secret => {
                    let owner = pending.owner;
                    let targets = pending.targets.clone();
                    if kde_secret(&String::from_utf8_lossy(&value)) {
                        Step::Secret { owner }
                    } else {
                        Step::Body { targets, owner }
                    }
                }
                Stage::Body(target) | Stage::Incremental { target, .. } => Step::Ready {
                    target: *target,
                    owner: pending.owner,
                    value,
                },
            }
        };
        match step {
            Step::Convert(target) => self.convert(target),
            Step::Body { targets, owner } => self.request_body(events, targets, owner),
            Step::Secret { owner } => self.finish_pending(
                events,
                Ok(Capture {
                    payload: None,
                    sensitive: true,
                    source_app: self.source_app(owner),
                }),
            ),
            Step::Ready {
                target,
                owner,
                value,
            } => self.finish_pending(
                events,
                Ok(Capture {
                    payload: payload_from_target(&self.atoms, target, &value),
                    sensitive: false,
                    source_app: self.source_app(owner),
                }),
            ),
        }
    }

    fn request_body(&mut self, events: &Sender<MonitorEvent>, targets: Vec<Atom>, owner: Window) {
        let Some(target) = preferred_target(&self.atoms, &targets, &self.kinds) else {
            self.finish_pending(
                events,
                Ok(Capture {
                    payload: None,
                    sensitive: false,
                    source_app: self.source_app(owner),
                }),
            );
            return;
        };
        if let Some(pending) = &mut self.pending {
            pending.stage = Stage::Body(target);
            pending.deadline = Instant::now() + Duration::from_millis(800);
        }
        self.convert(target);
    }

    fn convert(&mut self, target: Atom) {
        let _ = self
            .conn
            .convert_selection(
                self.window,
                self.atoms.clipboard,
                target,
                self.atoms.property,
                CURRENT_TIME,
            )
            .and_then(|cookie| cookie.check());
        let _ = self.conn.flush();
    }

    fn property(&self) -> Option<x11rb::protocol::xproto::GetPropertyReply> {
        self.conn
            .get_property(
                true,
                self.window,
                self.atoms.property,
                AtomEnum::ANY,
                0,
                8_000_000,
            )
            .ok()?
            .reply()
            .ok()
    }

    fn finish_pending(
        &mut self,
        events: &Sender<MonitorEvent>,
        result: Result<Capture, ClipboardError>,
    ) {
        if let Some(pending) = self.pending.take() {
            if let Some(sink) = pending.sink {
                let _ = sink.send(result);
            } else {
                match result {
                    Ok(capture) => {
                        let _ = events.send(MonitorEvent::Capture(capture));
                    }
                    Err(error) => {
                        let _ = events.send(MonitorEvent::Error(error.code));
                    }
                }
            }
        }
    }

    fn source_app(&self, owner: Window) -> Option<String> {
        if owner == 0 {
            return None;
        }
        let reply = self
            .conn
            .get_property(
                false,
                owner,
                self.atoms.net_wm_pid,
                AtomEnum::CARDINAL,
                0,
                1,
            )
            .ok()?
            .reply()
            .ok()?;
        let pid = u32::from_ne_bytes(reply.value.get(..4)?.try_into().ok()?);
        if pid == 0 {
            return None;
        }
        std::fs::read_link(format!("/proc/{pid}/exe"))
            .ok()
            .map(|path| path.to_string_lossy().into_owned())
            .or_else(|| {
                std::fs::read_to_string(format!("/proc/{pid}/comm"))
                    .ok()
                    .map(|name| name.trim().to_string())
            })
    }

    fn selection_request(&mut self, request: x11rb::protocol::xproto::SelectionRequestEvent) {
        let property = if request.property == NONE {
            request.target
        } else {
            request.property
        };
        let supported = self.supported_targets();
        let accepted = if request.target == self.atoms.targets {
            let _ = self.conn.change_property32(
                PropMode::REPLACE,
                request.requestor,
                property,
                AtomEnum::ATOM,
                &supported,
            );
            true
        } else if let Some(bytes) = self.bytes_for(request.target) {
            let type_ = if request.target == self.atoms.png {
                self.atoms.png
            } else if request.target == self.atoms.uri_list {
                self.atoms.uri_list
            } else {
                self.atoms.utf8
            };
            let _ = self.conn.change_property8(
                PropMode::REPLACE,
                request.requestor,
                property,
                type_,
                &bytes,
            );
            true
        } else {
            false
        };
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: if accepted { property } else { NONE },
        };
        let raw = notify.serialize();
        let mut padded = [0u8; 32];
        padded[..raw.len()].copy_from_slice(&raw);
        let _ = self
            .conn
            .send_event(false, request.requestor, EventMask::NO_EVENT, padded)
            .and_then(|cookie| cookie.check());
        let _ = self.conn.flush();
    }

    fn supported_targets(&self) -> Vec<Atom> {
        let mut targets = vec![self.atoms.targets];
        match &self.owned {
            Owned::None => {}
            Owned::Text(_) => {
                targets.extend([self.atoms.utf8, self.atoms.plain, AtomEnum::STRING.into()])
            }
            Owned::Png(_) => targets.push(self.atoms.png),
            Owned::Files { .. } => {
                targets.extend([self.atoms.uri_list, self.atoms.utf8, self.atoms.plain])
            }
        }
        targets
    }

    fn bytes_for(&self, target: Atom) -> Option<Vec<u8>> {
        match &self.owned {
            Owned::Text(bytes)
                if target == self.atoms.utf8
                    || target == self.atoms.plain
                    || target == AtomEnum::STRING.into() =>
            {
                Some(bytes.clone())
            }
            Owned::Png(bytes) if target == self.atoms.png => Some(bytes.clone()),
            Owned::Files { uri_list, text } if target == self.atoms.uri_list => {
                Some(uri_list.clone())
            }
            Owned::Files { text, .. }
                if target == self.atoms.utf8 || target == self.atoms.plain =>
            {
                Some(text.clone())
            }
            _ => None,
        }
    }
}

fn intern(conn: &RustConnection, name: &[u8]) -> Result<Atom, String> {
    conn.intern_atom(false, name)
        .map_err(|error| error.to_string())?
        .reply()
        .map(|reply| reply.atom)
        .map_err(|error| error.to_string())
}

fn atoms_from(value: &[u8]) -> Vec<Atom> {
    value
        .chunks_exact(4)
        .map(|chunk| u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn preferred_target(atoms: &Atoms, targets: &[Atom], kinds: &KindGate) -> Option<Atom> {
    let mut preferred = Vec::new();
    if kinds.files() {
        preferred.extend([atoms.gnome, atoms.uri_list]);
    }
    if kinds.image() {
        preferred.push(atoms.png);
    }
    if kinds.text() {
        preferred.extend([atoms.utf8, atoms.plain, AtomEnum::STRING.into()]);
    }
    preferred
        .into_iter()
        .find(|target| targets.contains(target))
}

fn payload_from_target(atoms: &Atoms, target: Atom, value: &[u8]) -> Option<Payload> {
    if target == atoms.png {
        return decode_png(value)
            .ok()
            .map(|(width, height, rgba)| Payload::Image {
                width,
                height,
                rgba,
            });
    }
    let text = String::from_utf8_lossy(value)
        .trim_end_matches('\0')
        .to_string();
    if target == atoms.gnome {
        let files = parse_gnome_copied(&text);
        return (!files.is_empty()).then_some(Payload::Files(files));
    }
    if target == atoms.uri_list {
        let files = parse_uri_list(&text);
        return (!files.is_empty()).then_some(Payload::Files(files));
    }
    (!text.is_empty()).then_some(Payload::Text(text))
}
