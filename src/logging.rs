use chrono::Local;
use log::{set_boxed_logger, set_max_level, Level, LevelFilter, Metadata, Record};
use serde::Deserialize;
use serde::Serialize;
use serde_json::to_string;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::prelude::FromRawFd;
use std::os::unix::prelude::RawFd;
use std::path::PathBuf;
use std::sync::Mutex;

enum LogFormat {
	TEXT,
	JSON,
}

#[derive(Serialize, Deserialize)]
pub struct LogEntry {
	pub level: String,
	pub msg: String,
	pub time: String,
}

struct RunhLogger<W: Write + Send + 'static> {
	log_file: Mutex<Option<W>>,
	log_file_internal: Mutex<Option<W>>,
	log_format: LogFormat,
}

impl<W: Write + Send + 'static> log::Log for RunhLogger<W> {
	fn enabled(&self, _metadata: &Metadata) -> bool {
		true
	}

	fn log(&self, record: &Record) {
		let mut file_lock = self.log_file.lock().unwrap();
		if self.enabled(record.metadata()) {
			let message = match self.log_format {
				LogFormat::TEXT => {
					format!("[{}] {}", record.level(), record.args())
				}
				LogFormat::JSON => to_string(&LogEntry {
					level: record.level().as_str().to_ascii_lowercase(),
					msg: format!("{}", record.args()),
					time: Local::now().to_rfc3339(),
				})
				.unwrap(),
			};
			if let Some(file) = &mut *file_lock {
				if let Err(err) = writeln!(file, "{}", message) {
					println!("ERROR in logger: {} Writing to stdout instead!", err);
					self.print_level(record.level());
					println!(" {}", record.args());
				}
			} else {
				self.print_level(record.level());
				println!(" {}", record.args());
			}
			let mut file_lock_backup = self.log_file_internal.lock().unwrap();
			if let Some(file_backup) = &mut *file_lock_backup {
				writeln!(file_backup, "{}", message).expect("Could not write to backup log file!");
			}
		}
	}

	fn flush(&self) {}
}

impl<W: Write + Send + 'static> RunhLogger<W> {
	/// To improve the readability, every log level
	/// get its own color. This helper function
	/// prints the log level with its associated color.
	fn print_level(&self, level: Level) {
		match level {
			Level::Info => {
				green!("[{}]", level);
			}
			Level::Debug => {
				blue!("[{}]", level);
			}
			Level::Error => {
				red!("[{}]", level);
			}
			Level::Warn => {
				yellow!("[{}]", level);
			}
			_ => {
				black!("{}", level);
			}
		}
	}
}

pub fn init(
	project_dir: PathBuf,
	log_path: Option<&str>,
	log_format: Option<&str>,
	log_level: Option<&str>,
	internal_log: bool,
) {
	let mut has_log_pipe = false;
	let log_file = log_path
		.map(|path| std::fs::File::create(path).expect("Could not create new log file!"))
		.or_else(|| {
			if let Ok(log_fd) = std::env::var("RUNH_LOG_PIPE") {
				let pipe_fd: i32 = log_fd.parse().expect("RUNH_LOG_PIPE was not an integer!");
				has_log_pipe = true;
				unsafe { Some(File::from_raw_fd(RawFd::from(pipe_fd))) }
			} else {
				None
			}
		});
	let log_format = log_format.map_or(LogFormat::TEXT, |fmt| match fmt {
		"json" => LogFormat::JSON,
		_ => LogFormat::TEXT,
	});

	let logger: RunhLogger<File> = RunhLogger {
		log_file: Mutex::new(log_file),
		log_file_internal: Mutex::new(if has_log_pipe || !internal_log {
			None
		} else {
			Some(
				OpenOptions::new()
					.create(true)
					.write(true)
					.open(project_dir.join(format!(
						"log-{}.json",
						Local::now().to_rfc3339().to_string()
					)))
					.expect("Could not open tmp log file!"),
			)
		}),
		log_format,
	};

	set_boxed_logger(Box::new(logger)).expect("Can't initialize logger");
	let max_level: LevelFilter = match log_level {
		Some("error") => LevelFilter::Error,
		Some("debug") => LevelFilter::Debug,
		Some("off") => LevelFilter::Off,
		Some("trace") => LevelFilter::Trace,
		Some("warn") => LevelFilter::Warn,
		Some("info") => LevelFilter::Info,
		_ => LevelFilter::Info,
	};
	set_max_level(max_level);

	debug!("Runh logger initialized!");
}
