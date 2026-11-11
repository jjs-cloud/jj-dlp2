//! Background file writer: coalesced JSON saves and appended log lines, so file I/O stays off the policy and GUI threads.

use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long the writer waits after the first job so a burst is written once.
const SETTLE: Duration = Duration::from_millis(200);
/// How long `flush` waits for the writer before giving up.
const FLUSH_WAIT: Duration = Duration::from_secs(10);

type Render = Box<dyn FnOnce() -> Option<Vec<u8>> + Send>;

struct Line {
    path: PathBuf,
    text: String,
    max_bytes: u64,
    trim: fn(&Path),
}

enum Job {
    Save(PathBuf, Render),
    Line(Line),
    Flush(Sender<()>),
}

static WRITER: OnceLock<Sender<Job>> = OnceLock::new();
static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn writer() -> &'static Sender<Job> {
    WRITER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("persist".into()).spawn(move || run(rx)).expect("spawn persist thread");
        tx
    })
}

fn submit(job: Job) {
    if cfg!(test) {
        handle(vec![job]);
    } else {
        let _ = writer().send(job);
    }
}

/// Queues a JSON save of `value`; a newer save of the same path replaces one still waiting.
pub fn save_json<T: Serialize + Send + 'static>(path: PathBuf, value: T, pretty: bool) {
    let render: Render = Box::new(move || {
        let bytes = if pretty { serde_json::to_vec_pretty(&value) } else { serde_json::to_vec(&value) };
        bytes.ok()
    });
    submit(Job::Save(path, render));
}

/// Queues text to append to `path`; `trim` runs on the file once it is over `max_bytes`.
pub fn append_line(path: PathBuf, text: String, max_bytes: u64, trim: fn(&Path)) {
    submit(Job::Line(Line { path, text, max_bytes, trim }));
}

/// Waits until everything queued so far is written.
pub fn flush() {
    let Some(tx) = WRITER.get() else { return };
    let (ack_tx, ack_rx) = mpsc::channel();
    if tx.send(Job::Flush(ack_tx)).is_ok() {
        let _ = ack_rx.recv_timeout(FLUSH_WAIT);
    }
}

/// Write failures since the last call.
pub fn take_errors() -> Vec<String> {
    std::mem::take(&mut *ERRORS.lock().unwrap())
}

fn report(msg: String) {
    let mut errors = ERRORS.lock().unwrap();
    if errors.last() != Some(&msg) {
        errors.push(msg);
    }
}

fn run(rx: mpsc::Receiver<Job>) {
    while let Ok(first) = rx.recv() {
        if !matches!(first, Job::Flush(_)) {
            std::thread::sleep(SETTLE);
        }
        let mut jobs = vec![first];
        jobs.extend(rx.try_iter());
        handle(jobs);
    }
}

fn handle(jobs: Vec<Job>) {
    let started = Instant::now();
    let mut saves: HashMap<PathBuf, Render> = HashMap::new();
    let mut lines: Vec<Line> = Vec::new();
    let mut acks = Vec::new();
    for job in jobs {
        match job {
            Job::Save(path, render) => {
                saves.insert(path, render);
            }
            Job::Line(line) => lines.push(line),
            Job::Flush(ack) => acks.push(ack),
        }
    }
    write_lines(lines);
    for (path, render) in saves {
        if let Some(bytes) = render() {
            if let Err(e) = write_atomic(&path, &bytes) {
                report(format!("failed to save {}: {e}", path.display()));
            }
        }
    }
    if started.elapsed() >= Duration::from_millis(100) {
        crate::perf::note(&format!("slow persist batch: {} ms", started.elapsed().as_millis()));
    }
    for ack in acks {
        let _ = ack.send(());
    }
}

/// Appends each path's lines in order with one write, then trims the file if it grew too large.
fn write_lines(lines: Vec<Line>) {
    let mut order: Vec<PathBuf> = Vec::new();
    let mut grouped: HashMap<PathBuf, (String, u64, fn(&Path))> = HashMap::new();
    for l in lines {
        let entry = grouped.entry(l.path.clone()).or_insert_with(|| {
            order.push(l.path.clone());
            (String::new(), l.max_bytes, l.trim)
        });
        entry.0.push_str(&l.text);
    }
    for path in order {
        let Some((text, max_bytes, trim)) = grouped.remove(&path) else { continue };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let written = std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|mut f| f.write_all(text.as_bytes()));
        if written.is_ok() && std::fs::metadata(&path).is_ok_and(|m| m.len() > max_bytes) {
            trim(&path);
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
