#![allow(dead_code, unused_imports)]
pub use anyhow::{anyhow, bail, Result};
use chrono::{NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use std::collections::HashSet;
pub use std::format as f;
use std::fs::{self, DirEntry};
pub use std::io::Write;
use std::io::{BufReader, Read};
use std::path::PathBuf;
pub use std::println as p;
use std::process::{Command, Stdio};
use std::time::Instant;
use std::{
    fs::File,
    path::Path,
    sync::{Mutex, OnceLock},
};

const TIME_FMT: &str = "%Y%m%d%H%M%S%6f";
const CURL_RETRY: [&str; 7] = [
    "--retry",
    "3",
    "--retry-all-errors",
    "--retry-delay",
    "0",
    "--retry-max-time",
    "30",
];

#[allow(dead_code)]
pub fn stopwatch_guard(name: &str) -> StopwatchGuard {
    let start = Instant::now();
    StopwatchGuard {
        name: name.to_string(),
        start,
    }
}

pub struct StopwatchGuard {
    name: String,
    start: Instant,
}

impl Drop for StopwatchGuard {
    fn drop(&mut self) {
        p!("{} took {}ms", self.name, self.start.elapsed().as_millis())
    }
}

#[macro_export]
macro_rules! stopwatch {
    () => {
        let ___stopwatch_guard = stopwatch_guard(&f!("fn at {}:{}", file!(), line!()));
    };
    ($e:expr) => {
        let ___stopwatch_guard = stopwatch_guard($e);
    };
}
pub(crate) use stopwatch;

pub fn fst<F, S>(x: (F, S)) -> F {
    x.0
}

pub trait NoWindow {
    fn no_window(&mut self) -> &mut Command;
}

impl NoWindow for Command {
    fn no_window(&mut self) -> &mut Command {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        // On non-Windows (macOS/Linux) do nothing — spawning without a console is the default.
        self
    }
}

pub fn snd<F, S>(x: (F, S)) -> S {
    x.1
}

pub trait Inspect<T, E> {
    fn tap(self, f: impl FnOnce(&Self)) -> Self;
}

pub trait InspectErr<T, E> {
    fn tap_err(self, f: impl FnOnce(&E)) -> Self;
}

impl<T, E> Inspect<T, E> for Result<T, E> {
    fn tap(self, f: impl FnOnce(&Result<T, E>)) -> Self {
        f(&self);
        self
    }
}

impl<T, E> InspectErr<T, E> for Result<T, E> {
    fn tap_err(self, f: impl FnOnce(&E)) -> Self {
        match self {
            Ok(_) => self,
            Err(ref err) => {
                f(err);
                self
            }
        }
    }
}

impl<T, E> Inspect<T, E> for Option<T> {
    fn tap(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
}

pub fn curl(
    url: &str,
    to: &str,
    proxy: Option<&str>,
    progress: &mut Option<impl FnMut(String, f64)>,
) -> Result<()> {
    let mut cmd = Command::new("curl");
    cmd.args(CURL_RETRY).args([url, "-k", "-#", "-o", to]);
    if let Some(proxy) = proxy {
        cmd.args(["-x", proxy]);
    }
    _ = dump!(&cmd);
    let mut output = cmd.stderr(Stdio::piped()).no_window().spawn()?;
    let mut ok = false;

    let mut start = Instant::now();
    if let Some(stderr) = output.stderr.take() {
        let mut stderr = BufReader::new(stderr);
        let mut buffer = [0; 1024];
        let rx = Regex::new(r".*?(\d+\.\d)%")?;
        while let Ok(n_bytes) = stderr.read(&mut buffer[..]) {
            if n_bytes == 0 {
                break;
            }
            let Ok(res) = String::from_utf8(buffer.into()) else {
                continue;
            };
            let Some(c) = rx.captures(&res) else { continue };
            let Some(p) = c.get(1) else { continue };
            ok = true;
            if let Some(progress) = progress {
                if start.elapsed().as_millis() > 500 {
                    start = Instant::now();
                    let p = p.as_str().parse::<f64>()?;
                    progress("Downloading assets".into(), p);
                }
            }
        }
    }
    if !ok {
        bail!("Error downloading {url}");
    }
    Ok(())
}

pub fn archive(
    source: impl AsRef<Path>,
    dest_file: impl AsRef<Path>,
    exclude: Option<&str>,
) -> Result<()> {
    let mut cmd = Command::new("tar");
    if let Some(exclude) = exclude {
        cmd.args(["--exclude", exclude]);
    }
    let (dir, file) = if source.as_ref().is_dir() {
        (source.as_ref(), "*")
    } else {
        (
            source
                .as_ref()
                .parent()
                .ok_or_err("can't get dir of file")?,
            source.as_ref().file_name().unwrap().to_str().unwrap(),
        )
    };
    cmd.args([
        "-czf",
        &dest_file.as_ref().to_string_lossy(),
        "-C",
        &dir.to_string_lossy(),
        file,
    ])
    .no_window()
    .status()?;
    Ok(())
}

pub trait ErrorExt<T> {
    fn ok_or_err(self, msg: &'static str) -> Result<T>;
}

impl<T> ErrorExt<T> for Option<T> {
    fn ok_or_err(self, msg: &'static str) -> Result<T> {
        self.ok_or_else(|| anyhow!(msg))
    }
}

#[macro_export]
macro_rules! dump {
    ($msg:expr, $args:expr) => {{
        log::info!("{} - {}: {:?}", $msg, function!(), $args);
        $args
    }};
    ($args:expr) => {
        dump!("DUMP", $args)
    };
}
pub(crate) use dump;

#[macro_export]
macro_rules! function {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        // Find and cut the rest of the path
        match &name[..name.len() - 3].rfind(':') {
            Some(pos) => &name[pos + 1..name.len() - 3],
            None => &name[..name.len() - 3],
        }
    }};
}
pub(crate) use function;

// pub fn capture_panics() {
//     std::panic::set_hook(Box::new(move |info| {
//         let msg = match info.payload().downcast_ref::<&'static str>() {
//             Some(s) => *s,
//             None => match info.payload().downcast_ref::<String>() {
//                 Some(s) => &**s,
//                 None => "Box<Any>",
//             },
//         };
//         match info.location() {
//             Some(location) => {
//                 l!("panic: '{}': {}:{}", msg, location.file(), location.line(),);
//             }
//             None => {
//                 l!("panic: '{}'", msg);
//             }
//         }
//     }));
// }

pub fn has_unique_elements<T, F, K>(iter: T, key: F) -> bool
where
    T: IntoIterator,
    F: Fn(&T::Item) -> K,
    K: Eq + core::hash::Hash,
{
    let mut uniq = HashSet::new();
    iter.into_iter().all(move |x| uniq.insert(key(&x)))
}

#[macro_export]
macro_rules! to_owned {
    ($($es:ident),+) => {$(
        #[allow(unused_mut)]
        let mut $es = $es.to_owned();
    )*}
}

use anyhow::Context;

pub trait ContextExt<T> {
    fn loc(self) -> anyhow::Result<T>;
    fn context<C>(self, context: C) -> anyhow::Result<T>
    where
        C: std::fmt::Display + Send + Sync + 'static;
}

impl<T, E: Into<anyhow::Error>> ContextExt<T> for Result<T, E> {
    #[track_caller]
    fn loc(self) -> anyhow::Result<T> {
        let location = std::panic::Location::caller();
        self.map_err(|e| {
            let e: anyhow::Error = e.into();
            e.context(format!(
                "@ {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            ))
        })
    }

    #[track_caller]
    fn context<C>(self, context: C) -> anyhow::Result<T>
    where
        C: std::fmt::Display + Send + Sync + 'static,
    {
        let location = std::panic::Location::caller();
        self.map_err(|e| {
            let e: anyhow::Error = e.into();
            e.context(format!(
                "{} @ {}:{}:{}",
                context,
                location.file(),
                location.line(),
                location.column()
            ))
        })
    }
}
