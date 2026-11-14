//! Low-volume diagnostics. Files rotate during a run, and panic reports
//! bypass the logger lock so a failure inside logging can still be recorded.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const KEPT_FILES: usize = 5;
const MAX_RECORD_BYTES: usize = 64 * 1024;
static LOGGER: OnceLock<FileLogger> = OnceLock::new();
static PANICS: OnceLock<FileLogger> = OnceLock::new();

struct FileLogger {
    path: PathBuf,
    file: Mutex<Option<File>>,
    limit: u64,
    keep: usize,
}

fn append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

impl FileLogger {
    fn new(dir: &Path, limit: u64, keep: usize) -> io::Result<Self> {
        Self::named(dir, "enso.log", limit, keep)
    }

    fn named(dir: &Path, name: &str, limit: u64, keep: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(name);
        Ok(Self {
            file: Mutex::new(Some(append(&path)?)),
            path,
            limit,
            keep,
        })
    }

    fn write(&self, line: &str) -> io::Result<()> {
        self.write_record(line, false)
    }

    fn write_record(&self, line: &str, durable: bool) -> io::Result<()> {
        let mut end = line.len().min(MAX_RECORD_BYTES);
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        let truncated = end != line.len();
        let line = &line[..end];
        let mut slot = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if slot
            .as_ref()
            .map(|f| f.metadata())
            .transpose()?
            .is_some_and(|m| m.len() + line.len() as u64 > self.limit)
        {
            drop(slot.take());
            let stem = self.path.file_stem().unwrap_or_default().to_string_lossy();
            let rotated = (|| {
                for n in (1..self.keep).rev() {
                    let from = if n == 1 {
                        self.path.clone()
                    } else {
                        self.path.with_file_name(format!("{stem}.{}.log", n - 1))
                    };
                    let to = self.path.with_file_name(format!("{stem}.{n}.log"));
                    if to.exists() {
                        fs::remove_file(&to)?;
                    }
                    if from.exists() {
                        fs::rename(from, to)?;
                    }
                }
                Ok::<_, io::Error>(())
            })();
            *slot = Some(append(&self.path)?);
            rotated?;
        }
        if slot.is_none() {
            *slot = Some(append(&self.path)?);
        }
        let file = slot.as_mut().expect("log file opened above");
        file.write_all(line.as_bytes())?;
        if truncated {
            file.write_all(b"... [truncated]\n")?;
        }
        file.flush()?;
        if durable {
            file.sync_data()?;
        }
        Ok(())
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
            || (metadata.level() <= log::Level::Info && metadata.target().starts_with("enso::"))
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} pid={} {} {} {}\n",
            timestamp(),
            std::process::id(),
            record.level(),
            record.target(),
            record.args()
        );
        if let Err(error) = self.write(&line) {
            eprintln!("Could not write enso log: {error}");
        }
    }

    fn flush(&self) {
        if let Some(file) = self.file.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = file.flush();
        }
    }
}

fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn directory() -> io::Result<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = dirs::home_dir().map(|p| p.join("Library/Logs"));
    #[cfg(target_os = "windows")]
    let base = dirs::data_local_dir();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = dirs::state_dir().or_else(dirs::data_local_dir);
    base.map(|p| p.join("enso"))
        .ok_or_else(|| io::Error::other("No local log directory is available"))
}

pub fn log_file() -> Option<&'static Path> {
    LOGGER.get().map(|l| l.path.as_path())
}

pub fn init() -> io::Result<()> {
    install(&directory()?)
}

fn install(dir: &Path) -> io::Result<()> {
    let logger = FileLogger::new(dir, MAX_BYTES, KEPT_FILES)?;
    let panics = FileLogger::named(dir, "enso-panic.log", MAX_BYTES, 2)?;
    LOGGER
        .set(logger)
        .map_err(|_| io::Error::other("Logging is already initialized"))?;
    PANICS
        .set(panics)
        .map_err(|_| io::Error::other("Panic logging is already initialized"))?;
    log::set_logger(LOGGER.get().expect("logger initialized above")).map_err(io::Error::other)?;
    log::set_max_level(log::LevelFilter::Info);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("Non-string panic");
        // Slice panics can quote the note. Preserve the bounds error while
        // keeping the document excerpt out of the persistent report.
        let message = payload.split("when slicing").next().unwrap_or(payload);
        let message: String = message.chars().take(2048).collect();
        let report = format!(
            "{} pid={} ERROR enso::panic thread={:?} location={} message={}\n{}\n",
            timestamp(),
            std::process::id(),
            std::thread::current().name(),
            info.location().map(|l| l.to_string()).unwrap_or_default(),
            message,
            std::backtrace::Backtrace::force_capture()
        );
        if let Some(logger) = PANICS.get() {
            let _ = logger.write_record(&report, true);
        }
        previous(info);
    }));
    log::info!(target: "enso::app", "app.start version={} os={} arch={}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    Ok(())
}

pub fn shutdown() {
    log::info!(target: "enso::app", "app.quit");
    log::logger().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_keeps_recent_records_and_bounds_file_count() {
        let dir = std::env::temp_dir().join(format!("enso-rotation-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("unrelated.txt"), "keep me").unwrap();
        let logger = FileLogger::new(&dir, 128, 3).unwrap();
        for n in 0..50 {
            logger
                .write(&format!("event {n:02}: {}\n", "x".repeat(40)))
                .unwrap();
        }
        assert!(
            fs::read_to_string(dir.join("enso.log"))
                .unwrap()
                .contains("event 49")
        );
        assert!(dir.join("enso.2.log").is_file());
        assert!(!dir.join("enso.3.log").exists());
        for name in ["enso.log", "enso.1.log", "enso.2.log"] {
            assert!(fs::metadata(dir.join(name)).unwrap().len() <= 128);
        }
        assert_eq!(
            fs::read_to_string(dir.join("unrelated.txt")).unwrap(),
            "keep me"
        );
        drop(logger);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn panic_child() {
        let Some(dir) = std::env::var_os("ENSO_TEST_PANIC_LOG") else {
            return;
        };
        install(Path::new(&dir)).unwrap();
        let note = Path::new(&dir).join("note.md");
        crate::files::save_as(&note, "PRIVATE_NOTE_SENTINEL").unwrap();
        crate::files::save(&note, "PRIVATE_NOTE_SENTINEL", "changed").unwrap();
        assert!(crate::files::save(&note, "wrong expected text", "new text").is_err());
        // Hold the logger lock: the panic hook must still write its report.
        let _lock = LOGGER.get().unwrap().file.lock().unwrap();
        panic!("intentional panic for diagnostic test when slicing PRIVATE_NOTE_SENTINEL");
    }

    #[test]
    fn a_panic_is_persisted_even_with_the_logger_locked() {
        let dir = std::env::temp_dir().join(format!("enso-panic-log-{}", std::process::id()));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "logging::tests::panic_child", "--nocapture"])
            .env("ENSO_TEST_PANIC_LOG", &dir)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let events = fs::read_to_string(dir.join("enso.log")).unwrap();
        for event in [
            "app.start",
            "file.create.complete",
            "file.save.complete",
            "file.save.failed",
        ] {
            assert!(events.contains(event), "missing {event}");
        }
        assert!(!events.contains("PRIVATE_NOTE_SENTINEL"));
        let report = fs::read_to_string(dir.join("enso-panic.log")).unwrap();
        assert!(report.contains("intentional panic for diagnostic test"));
        assert!(report.contains("enso::panic") && report.contains("logging.rs:"));
        assert!(!report.contains("PRIVATE_NOTE_SENTINEL"));
        fs::remove_dir_all(dir).unwrap();
    }
}
