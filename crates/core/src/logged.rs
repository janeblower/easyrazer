//! Records every exchange on a transport, for the app's developer mode.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::packet::{self, Response, Status};
use crate::transport::{Error, MAX_POLLS, Report, Transport};

/// Entries kept unless [`Log::set_capacity`] says otherwise.
pub const CAPACITY: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Entry {
    pub seq: u64,
    /// Unix time, ms.
    pub time: u64,
    /// `CC:II`, or a label for traffic outside the control interface.
    pub cmd: String,
    pub size: u8,
    pub args: Vec<u8>,
    /// Status of the last reply; `None` when none came.
    pub status: Option<String>,
    pub reply: Vec<u8>,
    /// Busy replies before the last one.
    pub polls: u8,
    pub ms: u32,
    pub error: Option<String>,
    /// Made by the app's own polling.
    pub background: bool,
    /// Whole reports without the report ID.
    pub tx: Vec<u8>,
    pub rx: Vec<u8>,
}

pub type Listener = Box<dyn Fn(&Entry) + Send + Sync>;

pub struct Log {
    entries: Mutex<VecDeque<Entry>>,
    capacity: AtomicUsize,
    seq: AtomicU64,
    background: AtomicBool,
    listener: Mutex<Option<Listener>>,
}

impl Default for Log {
    fn default() -> Self {
        Self { entries: Mutex::default(), capacity: AtomicUsize::new(CAPACITY), seq: AtomicU64::default(), background: AtomicBool::default(), listener: Mutex::default() }
    }
}

impl Log {
    /// Drops the oldest entries beyond `n`, at least one entry is kept.
    pub fn set_capacity(&self, n: usize) {
        let n = n.max(1);
        self.capacity.store(n, Ordering::Relaxed);
        let mut q = lock(&self.entries);
        let excess = q.len().saturating_sub(n);
        q.drain(..excess);
    }

    pub fn entries(&self) -> Vec<Entry> {
        lock(&self.entries).iter().cloned().collect()
    }

    pub fn last(&self) -> Option<Entry> {
        lock(&self.entries).back().cloned()
    }

    pub fn listen(&self, listener: Option<Listener>) {
        *lock(&self.listener) = listener;
    }

    /// Marks the exchanges made meanwhile as the app's own polling.
    pub fn set_background(&self, on: bool) {
        self.background.store(on, Ordering::Relaxed);
    }

    /// Records a report sent outside a [`Logged`] transport.
    pub fn note(&self, cmd: &str, args: &[u8], error: Option<String>) {
        self.push(Entry { cmd: cmd.into(), size: args.len() as u8, args: args.to_vec(), error, tx: args.to_vec(), ..Default::default() });
    }

    fn push(&self, mut e: Entry) {
        e.seq = self.seq.fetch_add(1, Ordering::Relaxed);
        e.time = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        e.background = self.background.load(Ordering::Relaxed);
        if let Some(l) = &*lock(&self.listener) {
            l(&e);
        }
        let mut q = lock(&self.entries);
        if q.len() >= self.capacity.load(Ordering::Relaxed) {
            q.pop_front();
        }
        q.push_back(e);
    }
}

/// A listener that panicked must not stop the logging.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Pending {
    start: Instant,
    tx: [u8; packet::LEN],
    polls: u8,
}

pub struct Logged<T> {
    inner: T,
    log: Arc<Log>,
    pending: RefCell<Option<Pending>>,
}

impl<T> Logged<T> {
    pub fn new(inner: T, log: Arc<Log>) -> Self {
        Self { inner, log, pending: RefCell::new(None) }
    }

    fn finish(&self, rx: Option<&[u8; packet::LEN]>, error: Option<String>) {
        let Some(p) = self.pending.borrow_mut().take() else { return };
        let req = Response::parse(&p.tx);
        let reply = rx.map(Response::parse);
        self.log.push(Entry {
            cmd: req.cmd.to_string(),
            size: req.size,
            args: req.data().to_vec(),
            status: reply.as_ref().map(|r| format!("{:?}", r.status)),
            reply: reply.as_ref().map(|r| r.data().to_vec()).unwrap_or_default(),
            polls: p.polls,
            ms: p.start.elapsed().as_millis() as u32,
            error,
            tx: p.tx.to_vec(),
            rx: rx.map(|r| r.to_vec()).unwrap_or_default(),
            ..Default::default()
        });
    }
}

impl<T: Transport> Transport for Logged<T> {
    fn send_feature(&self, report: &Report) -> Result<(), Error> {
        *self.pending.borrow_mut() = Some(Pending { start: Instant::now(), tx: report[1..].try_into().unwrap(), polls: 0 });
        let r = self.inner.send_feature(report);
        if let Err(e) = &r {
            self.finish(None, Some(e.to_string()));
        }
        r
    }

    fn get_feature(&self, report: &mut Report) -> Result<(), Error> {
        let r = self.inner.get_feature(report);
        let rx: &[u8; packet::LEN] = report[1..].try_into().unwrap();
        match &r {
            Err(e) => self.finish(None, Some(e.to_string())),
            Ok(()) if Status::from(rx[0]) == Status::Busy => {
                let polls = self.pending.borrow_mut().as_mut().map(|p| {
                    p.polls += 1;
                    p.polls
                });
                // `exchange` gives up after this many and reads no more.
                if polls == Some(MAX_POLLS as u8) {
                    self.finish(Some(rx), Some("device stayed busy".into()));
                }
            }
            Ok(()) => self.finish(Some(rx), None),
        }
        r
    }

    fn tid(&self) -> u8 {
        self.inner.tid()
    }
}

/// Bytes of the args and of the reply a bug report may show; the rest can hold a macro's keys, a profile name or the serial.
pub fn public_len(e: &Entry) -> (usize, usize) {
    let (args, reply) = match e.cmd.as_str() {
        "06:09" => (7, 7),
        "05:08" => (5, 5),
        "05:88" => (usize::MAX, 5),
        "00:82" => (usize::MAX, 0),
        _ => (usize::MAX, usize::MAX),
    };
    (args.min(e.args.len()), reply.min(e.reply.len()))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::fake::FakeKeyboard;
    use crate::packet::Command;
    use crate::transport::{TID, exchange};
    use crate::{control, macros};

    const MODE: Command = Command::new(0x00, 0x84);

    fn logged<T>(t: T) -> Logged<T> {
        Logged::new(t, Arc::new(Log::default()))
    }

    /// Replies popped from the end.
    struct Scripted(RefCell<Vec<Report>>);

    impl Transport for Scripted {
        fn send_feature(&self, _: &Report) -> Result<(), Error> {
            Ok(())
        }

        fn get_feature(&self, r: &mut Report) -> Result<(), Error> {
            *r = self.0.borrow_mut().pop().ok_or(Error::Io("no reply".into()))?;
            Ok(())
        }
    }

    fn reply(status: u8) -> Report {
        let mut r = [0u8; packet::LEN + 1];
        r[1..].copy_from_slice(&packet::request(TID, MODE, 2, &[3, 0]));
        r[1] = status;
        r
    }

    #[test]
    fn records_one_entry_per_exchange() {
        let t = logged(FakeKeyboard::new(&[31]));
        control::mode(&t).unwrap();
        let e = t.log.entries();
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].cmd.as_str(), e[0].size, e[0].status.as_deref()), ("00:84", 2, Some("Ok")));
        assert_eq!(e[0].reply[0], control::MODE_DRIVER);
        assert_eq!((e[0].tx.len(), e[0].rx.len()), (packet::LEN, packet::LEN));
    }

    #[test]
    fn records_what_the_device_refused() {
        let t = logged(FakeKeyboard::new(&[31]));
        assert!(exchange(&t, Command::new(0x7F, 0x7F), 1, &[]).is_err());
        assert_eq!(t.log.entries()[0].status.as_deref(), Some("NotSupported"));
    }

    #[test]
    fn busy_replies_count_as_polls() {
        let t = logged(Scripted(RefCell::new(vec![reply(0x02), reply(0x01), reply(0x01)])));
        exchange(&t, MODE, 2, &[]).unwrap();
        let e = t.log.entries();
        assert_eq!((e.len(), e[0].polls, e[0].status.as_deref()), (1, 2, Some("Ok")));
    }

    #[test]
    fn a_device_that_stays_busy_is_recorded_once() {
        let t = logged(Scripted(RefCell::new(vec![reply(0x01); MAX_POLLS])));
        assert!(exchange(&t, MODE, 2, &[]).is_err());
        let e = t.log.entries();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].error.as_deref(), Some("device stayed busy"));
    }

    #[test]
    fn records_a_failed_read() {
        let t = logged(Scripted(RefCell::new(Vec::new())));
        assert!(exchange(&t, MODE, 2, &[]).is_err());
        assert_eq!(t.log.entries()[0].error.as_deref(), Some("HID: no reply"));
    }

    #[test]
    fn keeps_the_last_entries() {
        let t = logged(FakeKeyboard::new(&[31]));
        for _ in 0..CAPACITY + 5 {
            control::mode(&t).unwrap();
        }
        let e = t.log.entries();
        assert_eq!((e.len(), e[0].seq), (CAPACITY, 5));
    }

    #[test]
    fn capacity_can_grow_and_shrink() {
        let t = logged(FakeKeyboard::new(&[31]));
        t.log.set_capacity(CAPACITY + 10);
        for _ in 0..CAPACITY + 5 {
            control::mode(&t).unwrap();
        }
        assert_eq!(t.log.entries().len(), CAPACITY + 5);
        t.log.set_capacity(3);
        let e = t.log.entries();
        assert_eq!((e.len(), e[0].seq), (3, CAPACITY as u64 + 2), "the newest stay");
    }

    #[test]
    fn the_listener_sees_each_entry_with_its_background_flag() {
        let t = logged(FakeKeyboard::new(&[31]));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        t.log.listen(Some(Box::new(move |e| s.lock().unwrap().push(e.background))));
        t.log.set_background(true);
        control::mode(&t).unwrap();
        t.log.set_background(false);
        control::mode(&t).unwrap();
        t.log.note("LampArray autonomous", &[1], None);
        assert_eq!(*seen.lock().unwrap(), [true, false, false]);
    }

    #[test]
    fn a_report_shows_only_headers_of_macro_bodies_and_profile_names() {
        let t = logged(FakeKeyboard::new(&[31]));
        macros::write(&t, 3, &[0xAA; 10]).unwrap();
        let body = t.log.entries().into_iter().find(|e| e.cmd == "06:09").unwrap();
        assert_eq!(public_len(&body), (7, 7));
        let name = Entry { cmd: "05:88".into(), args: vec![1, 0, 0, 0, 0x40], reply: vec![0; 20], ..Default::default() };
        assert_eq!(public_len(&name), (5, 5));
        let set = Entry { cmd: "05:08".into(), args: vec![0; 12], reply: vec![0; 12], ..Default::default() };
        assert_eq!(public_len(&set), (5, 5));
        let serial = Entry { cmd: "00:82".into(), args: vec![1], reply: b"PM2149H00012345".to_vec(), ..Default::default() };
        assert_eq!(public_len(&serial), (1, 0));
        let other = Entry { cmd: "00:84".into(), args: vec![1], reply: vec![2, 3], ..Default::default() };
        assert_eq!(public_len(&other), (1, 2));
    }
}
