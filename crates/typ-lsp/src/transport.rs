//! A server process, two threads, and the guarantee that it dies when we do.
//!
//! `lsp-server` is described as a server scaffold, and `Connection::stdio` is.
//! But `Message::read` and `Message::write` are generic over `BufRead` and
//! `Write`, so they frame just as well over a child's pipes — which is what
//! makes rust-analyzer's own transport usable from the client side.
//!
//! **Killing the child is not enough.** rust-analyzer spawns `cargo`, which
//! spawns `rustc`. If TYPE dies badly — a panic, a SIGKILL, a closed terminal
//! window — killing only the direct child leaves that subtree running and a
//! core busy indefinitely. So the child goes into a Windows job object, or a
//! Unix process group, and the whole tree goes at once.

use std::io::{BufReader, Write};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use lsp_server::Message;

/// Why a server could not be started.
///
/// Returned rather than logged: nothing below `typ-app` can log, so a lower
/// crate reports a reason by handing it back as data. `ParseError` exists for
/// the same reason and for no other.
#[derive(Debug)]
pub enum SpawnError {
    /// The command is not on `PATH`. The ordinary case on most machines, and
    /// not something the user should be shown as an error.
    NotFound { command: String },
    /// It exists and would not start.
    Failed {
        command: String,
        source: std::io::Error,
    },
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::NotFound { command } => write!(f, "{command} is not on PATH"),
            SpawnError::Failed { command, source } => {
                write!(f, "{command} would not start: {source}")
            }
        }
    }
}

impl std::error::Error for SpawnError {}

/// Something the server said.
#[derive(Debug)]
pub enum Incoming {
    /// A framed protocol message.
    Message(Box<Message>),
    /// The connection ended. Always the last thing sent, exactly once.
    Closed,
}

/// How many lines of the server's stderr to keep.
///
/// Enough to say why it died, bounded so a server that logs forever cannot
/// grow the editor's memory without limit.
const STDERR_LINES: usize = 32;

/// How many unparsable frames in a row before the connection is given up.
///
/// Small on purpose. One is a server bug worth surviving; a run of them means
/// the stream is desynchronised and nothing after it can be trusted.
const MAX_CONSECUTIVE_ERRORS: usize = 8;

/// A running server, its threads, and its lifetime.
pub struct Transport {
    child: Child,
    /// Dropped to tell the writer thread to stop.
    outgoing: Option<Sender<Message>>,
    stderr: Arc<Mutex<Vec<String>>>,
    #[cfg(windows)]
    job: platform::Job,
}

impl Transport {
    /// Start `command` in `root` and begin pumping both directions.
    ///
    /// Generic over the result type so this crate never names the app's event
    /// enum — the same arrangement `ParseWorker::spawn` uses, and for the same
    /// reason: a dev-dependency on `typ-core` would be a publish-order failure
    /// waiting for release day.
    pub fn spawn<E>(
        command: &str,
        args: &[String],
        root: &std::path::Path,
        results: Sender<E>,
    ) -> Result<Transport, SpawnError>
    where
        E: From<Incoming> + Send + 'static,
    {
        let mut builder = Command::new(command);
        builder
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        platform::detach(&mut builder);

        let mut child = builder.spawn().map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                SpawnError::NotFound {
                    command: command.to_string(),
                }
            } else {
                SpawnError::Failed {
                    command: command.to_string(),
                    source,
                }
            }
        })?;

        #[cfg(windows)]
        let job = platform::confine(&child);

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let (tx, rx) = mpsc::channel::<Message>();

        thread::Builder::new()
            .name("typ-lsp-write".into())
            .spawn(move || {
                let mut stdin = stdin;
                // `recv` fails once the Transport holding the sender is
                // dropped, which is how this thread learns to exit.
                while let Ok(message) = rx.recv() {
                    if message.write(&mut stdin).is_err() || stdin.flush().is_err() {
                        break;
                    }
                }
            })
            .expect("the OS can start a thread");

        thread::Builder::new()
            .name("typ-lsp-read".into())
            .spawn(move || {
                let mut stdout = BufReader::new(stdout);
                let mut consecutive_errors = 0usize;
                loop {
                    match Message::read(&mut stdout) {
                        Ok(Some(message)) => {
                            consecutive_errors = 0;
                            let boxed = Box::new(message);
                            if results.send(E::from(Incoming::Message(boxed))).is_err() {
                                return;
                            }
                        }
                        // End of stream. The server is done.
                        Ok(None) => break,
                        // A frame that would not parse, and we cannot tell
                        // which kind. `read_msg_text` reads the headers, then
                        // `read_exact`s the body — so a body that is not JSON
                        // leaves the stream synchronised and the next frame is
                        // fine, while a malformed *header* leaves it
                        // desynchronised and nothing after it will parse.
                        // Both arrive as `InvalidData`, and discriminating on
                        // the message text would break the first time upstream
                        // rewords it. A bounded retry is right either way: one
                        // bad body costs a message, and a desynchronised stream
                        // exhausts the budget and closes.
                        Err(_) => {
                            consecutive_errors += 1;
                            if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                break;
                            }
                        }
                    }
                }
                let _ = results.send(E::from(Incoming::Closed));
            })
            .expect("the OS can start a thread");

        let tail = Arc::new(Mutex::new(Vec::new()));
        drain_stderr(stderr, Arc::clone(&tail));

        Ok(Transport {
            child,
            outgoing: Some(tx),
            stderr: tail,
            #[cfg(windows)]
            job,
        })
    }

    /// Queue a message. Never blocks, and never fails visibly: a server that
    /// has gone away is reported by the reader thread's `Closed`, once.
    pub fn send(&self, message: Message) {
        if let Some(tx) = &self.outgoing {
            let _ = tx.send(message);
        }
    }

    /// The child's process id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The last lines the server wrote to stderr.
    ///
    /// Data, not a log: this crate has nobody to tell. `typ-app` decides
    /// whether a dead server's last words are worth showing.
    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr.lock().map(|t| t.clone()).unwrap_or_default()
    }

    /// Stop the writer thread, so the server sees end of input.
    ///
    /// The polite half of shutting down: a server told `exit` wants its stdin
    /// closed, and one that was not told will see EOF and stop on its own.
    /// Killing is what happens when neither works.
    pub fn close_input(&mut self) {
        self.outgoing = None;
    }

    /// Wait briefly for the server to exit on its own. Returns whether it did.
    ///
    /// rust-analyzer writes state on shutdown, and killing it every time is how
    /// a stale lock outlives the editor.
    pub fn wait_for_exit(&mut self, within: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + within;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => {}
                Err(_) => return false,
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.outgoing = None;
        platform::kill_tree(self);
        let _ = self.child.wait();
    }
}

fn drain_stderr(stderr: ChildStderr, tail: Arc<Mutex<Vec<String>>>) {
    thread::Builder::new()
        .name("typ-lsp-stderr".into())
        .spawn(move || {
            use std::io::BufRead;
            // Drained rather than ignored: an undrained pipe fills its buffer
            // and blocks the server on its next write, which looks exactly like
            // a hang and is not one.
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Ok(mut tail) = tail.lock() {
                    if tail.len() == STDERR_LINES {
                        tail.remove(0);
                    }
                    tail.push(line);
                }
            }
        })
        .expect("the OS can start a thread");
}

#[cfg(unix)]
mod platform {
    use super::Transport;
    use std::os::unix::process::CommandExt;

    /// Put the child in its own process group, so signalling the group reaches
    /// everything it goes on to spawn.
    pub fn detach(builder: &mut std::process::Command) {
        builder.process_group(0);
    }

    pub fn kill_tree(transport: &mut Transport) {
        let pid = transport.child.id() as i32;
        // A negative pid means the process group. That is the whole point: the
        // grandchildren are in it too.
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        if transport.wait_for_exit(std::time::Duration::from_millis(200)) {
            return;
        }
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::Transport;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };

    /// A job object holding the server and everything it spawns.
    ///
    /// `KILL_ON_JOB_CLOSE` is the guarantee that matters, and it is stronger
    /// than anything `Drop` can promise: when the last handle to the job closes
    /// — including when TYPE is killed outright and Windows closes it on our
    /// behalf — every process in the job dies with it.
    pub struct Job(HANDLE);

    // The handle is owned by this struct and closed only in `Drop`.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Drop for Job {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    /// Nothing to do before spawning; the job is assigned afterwards.
    pub fn detach(_builder: &mut std::process::Command) {}

    pub fn confine(child: &Child) -> Job {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Job(std::ptr::null_mut());
            }

            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );

            AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE);
            Job(job)
        }
    }

    pub fn kill_tree(transport: &mut Transport) {
        if transport.wait_for_exit(std::time::Duration::from_millis(200)) {
            return;
        }
        if !transport.job.0.is_null() {
            unsafe { TerminateJobObject(transport.job.0, 1) };
        } else {
            let _ = transport.child.kill();
        }
    }
}
