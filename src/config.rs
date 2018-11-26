//! Configuration.

use std::{cmp, env, fmt, fs, io, ops, process};
use std::io::{Read, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use clap::{App, Arg, ArgMatches};
use dirs::home_dir;
use log::LevelFilter;
use toml;


//------------ Config --------------------------------------------------------

/// Routinator configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// Path to the directory that contains the repository cache.
    pub cache_dir: PathBuf,

    /// Path to the directory that contains the trust anchor locators.
    pub tal_dir: PathBuf,

    /// Path to the optional local exceptions file.
    pub exceptions: Option<PathBuf>,

    /// Expected mode of operation.
    pub mode: RunMode,

    /// Path to the output file.
    ///
    /// If this is `None`, we are supposed to output to stdout. 
    pub output: Option<PathBuf>,

    /// Format for output to a file.
    pub outform: OutputFormat,

    /// Should we do strict validation?
    pub strict: bool,

    /// Should we update the repository cache?
    pub update: bool,

    /// Should we process the repository?
    pub process: bool,

    /// The log level filter for setting up logging.
    pub verbose: LevelFilter,

    /// The refresh interval for repository validation.
    pub refresh: Duration,

    pub retry: Duration,

    pub expire: Duration,

    /// How many diffs to keep in the history.
    pub history_size: usize,

    /// Addresses to listen for RTR connections on.
    pub rtr_listen: Vec<SocketAddr>,
}

impl Config {
    pub fn create() -> Self {
        let args = Args::create();

        if args.is_present("man") {
            let stdout = io::stdout();
            let _ = stdout.lock().write_all(MAN_PAGE);
            process::exit(0);
        }

        let mut file = args.path_value_of("config").as_ref().map(|path| {
            File::read(path)
        });

        let (cache_dir, tal_dir) = Self::prepare_dirs(
            &args, file.as_mut()
        );

        let mut file = file.unwrap_or_default();

        let listen = match args.values_of("listen") {
            Some(values) => {
                let mut listen = Vec::new();
                for val in values {
                    match val.to_socket_addrs() {
                        Ok(some) => listen.extend(some),
                        Err(_) => {
                            println!("Invalid socket address {}", val);
                            process::exit(1);
                        }
                    }
                }
                listen
            }
            None => {
                match file.take_str_vec("listen") {
                    Some(v) => v,
                    None => {
                        vec![SocketAddr::from_str("127.0.0.1:3323").unwrap()]
                    }
                }
            }
        };

        Config {
            cache_dir,
            tal_dir,
            exceptions: {
                args.path_value_of("exceptions")
                    .or_else(|| file.take_path("exceptions"))
            },
            mode: if args.is_present("daemon") {
                RunMode::Daemon
            }
            else if args.is_present("repeat") {
                RunMode::Repeat
            }
            else {
                RunMode::Once
            },
            output: {
                if args.value_of("output") == Some("-") {
                    None
                } else {
                    args.path_value_of("output")
                }
            },
            outform: match args.value_of("outform") {
                Some("csv") => OutputFormat::Csv,
                Some("json") => OutputFormat::Json,
                Some("rpsl") => OutputFormat::Rpsl,
                Some("none") => OutputFormat::None,
                Some(_) => {
                    // This should be covered by clap above.
                    unreachable!();
                }
                None => OutputFormat::Csv,
            },
            strict: {
                file.take_bool("strict").unwrap_or(false) ||
                args.is_present("strict")
            },
            update: !args.is_present("noupdate"),
            process: !args.is_present("noprocess"),
            verbose: {
                let arg = match args.occurrences_of("verbose") {
                    0 => LevelFilter::Error,
                    1 => LevelFilter::Info,
                    _ => LevelFilter::Debug,
                };
                let opt = file.take_from_str("log-level")
                    .unwrap_or(LevelFilter::Error);
                cmp::max(arg, opt)
            },
            refresh: {
                let value = args.value_of("refresh").unwrap();
                match u64::from_str(value) {
                    Ok(some) => Duration::from_secs(some),
                    Err(_) => {
                        error!(
                            "Invalid value '{}' for refresh argument.\
                             Needs to be number of seconds.",
                            value
                        );
                        process::exit(1);
                    }
                }
            },
            retry: {
                Duration::from_secs(file.take_uint("retry").unwrap_or(600))
            },
            expire: {
                Duration::from_secs(file.take_uint("expire").unwrap_or(7200))
            },
            history_size: {
                args.from_str_value_of("history").unwrap_or_else(|| {
                    match file.take_from_str("history") {
                        Some(value) => value,
                        None => 10
                    }
                })
            },
            rtr_listen: listen,
        }
    }


    /// Prepares and returns the cache dir and tal dir.
    fn prepare_dirs(
        args: &Args,
        file: Option<&mut File>,
    ) -> (PathBuf, PathBuf) {
        let base_dir = args.path_value_of("basedir");
        let cache_dir = match args.path_value_of("cachedir") {
            Some(dir) => Some(dir),
            None => match base_dir {
                Some(ref dir) => Some(dir.join("repository")),
                None => None
            },
        };
        let tal_dir = match args.path_value_of("taldir") {
            Some(dir) => Some(dir),
            None => match base_dir {
                Some(ref dir) => Some(dir.join("tals")),
                None => None
            }
        };
        let (cache_dir, tal_dir) = if cache_dir.is_none() || tal_dir.is_none() {
            if let Some(file) = file {
                let (c, t) = file.take_dirs();
                (cache_dir.unwrap_or(c), tal_dir.unwrap_or(t))
            }
            else {
                let base_dir = match home_dir() {
                    Some(dir) => dir,
                    None => {
                        println!(
                            "Cannot get home directory. \
                             Please specify directories with -b, -c, and -t \
                             options."
                        );
                        process::exit(1);
                    }
                };
                (
                    cache_dir.unwrap_or_else(|| base_dir.join("repository")),
                    tal_dir.unwrap_or_else(|| base_dir.join("tals"))
                )
            }
        }
        else {
            (cache_dir.unwrap(), tal_dir.unwrap())
        };

        if let Err(err) = fs::create_dir_all(&cache_dir) {
            println!(
                "Can't create repository directory {}: {}.\nAborting.",
                cache_dir.display(), err
            );
            process::exit(1);
        }
        if fs::read_dir(&tal_dir).is_err() {
            if let Err(err) = fs::create_dir_all(&tal_dir) {
                println!(
                    "Can't create TAL directory {}: {}.\nAborting.",
                    tal_dir.display(), err
                );
                process::exit(1);
            }
            for (name, content) in &DEFAULT_TALS {
                let mut file = match fs::File::create(tal_dir.join(name)) {
                    Ok(file) => file,
                    Err(err) => {
                        println!(
                            "Can't create TAL file {}: {}.\n Aborting.",
                            tal_dir.join(name).display(), err
                        );
                        process::exit(1);
                    }
                };
                if let Err(err) = file.write_all(content) {
                    println!(
                        "Can't create TAL file {}: {}.\n Aborting.",
                        tal_dir.join(name).display(), err
                    );
                    process::exit(1);
                }
            }
        }

        (cache_dir, tal_dir)
    }
}


//------------ Args ----------------------------------------------------------

struct Args<'a> {
    matches: ArgMatches<'a>,
    dir: PathBuf,
}

impl<'a> Args<'a> {
    fn create() -> Self {
        // Remember to update the man page if you change things here!
        let matches = App::new("Routinator")
            .version(crate_version!())
            .author(crate_authors!())
            .about("validates RPKI route origin attestations")

            .arg(Arg::with_name("config")
                 .short("c")
                 .long("config")
                 .value_name("FILE")
                 .help("config file")
                 .takes_value(true)
            )

            .arg(Arg::with_name("basedir")
                 .short("b")
                 .long("base-dir")
                 .value_name("DIR")
                 .help("sets the base directory for cache and TALs")
                 .takes_value(true)
            )
            .arg(Arg::with_name("cachedir")
                 .short("c")
                 .long("cache-dir")
                 .value_name("DIR")
                 .help("sets the cache directory")
                 .takes_value(true)
            )
            .arg(Arg::with_name("taldir")
                 .short("t")
                 .long("tal-dir")
                 .value_name("DIR")
                 .help("sets the TAL directory")
                 .takes_value(true)
            )
            .arg(Arg::with_name("exceptions")
                 .short("x")
                 .long("exceptions")
                 .value_name("FILE")
                 .help("file with local exceptions (see RFC 8416 for format)")
                 .takes_value(true)
            )
            .arg(Arg::with_name("daemon")
                 .short("d")
                 .long("daemon")
                 .help("run in daemon mode (detach from terminal)")
            )
            .arg(Arg::with_name("repeat")
                 .short("r")
                 .long("repeat")
                 .help("repeatedly run validation")
            )
            .arg(Arg::with_name("output")
                 .short("o")
                 .long("output")
                 .value_name("FILE")
                 .help("output file, '-' or not present for stdout")
                 .takes_value(true)
            )
            .arg(Arg::with_name("outform")
                 .short("f")
                 .long("outform")
                 .value_name("FORMAT")
                 .possible_values(&["csv", "json", "rpsl", "none"])
                 //.help("sets the output format (csv, json, rpsl, none)")
                 .help("sets the output format")
                 .takes_value(true)
            )
            .arg(Arg::with_name("listen")
                 .short("l")
                 .long("listen")
                 .value_name("ADDR:PORT")
                 .help("listen addr:port for RTR.")
                 .takes_value(true)
                 .multiple(true)
            )
            .arg(Arg::with_name("noupdate")
                 .short("n")
                 .long("noupdate")
                 .help("don't update local cache")
            )
            .arg(Arg::with_name("noprocess")
                 .short("N")
                 .long("noprocess")
                 .help("don't process the repository")
            )
            .arg(Arg::with_name("strict")
                 .long("strict")
                 .help("parse RPKI data in strict mode")
            )
            .arg(Arg::with_name("refresh")
                 .long("refresh")
                 .value_name("SECONDS")
                 .default_value("3600")
                 .help("refresh interval in seconds")
            )
            .arg(Arg::with_name("history_size")
                 .long("history")
                 .value_name("COUNT")
                 .help("number of history items to keep in repeat mode")
            )
            .arg(Arg::with_name("verbose")
                 .short("v")
                 .long("verbose")
                 .multiple(true)
                 .help("print more (and more) information")
            )
            .arg(Arg::with_name("man")
                 .long("man")
                 .help("print the man page to stdout")
            )
            .get_matches();
        let dir = match env::current_dir() {
            Ok(dir) => dir,
            Err(err) => {
                println!(
                    "Fatal: cannot get current directory ({}). Aborting.",
                    err
                );
                process::exit(1);
            }
        };
        Args { matches, dir }
    }

    fn path_value_of(&self, key: &str) -> Option<PathBuf> {
        self.matches.value_of(key).map(|v| self.dir.join(v))
    }

    fn from_str_value_of<V>(&self, key: &str) -> Option<V>
    where V: FromStr, V::Err: fmt::Display {
        self.matches.value_of(key).map(|v| {
            match V::from_str(v) {
                Ok(some) => some,
                Err(err) => {
                    println!("Invalid value for '{}' argument: {}", key, err);
                    process::exit(1);
                }
            }
        })
    }
}

impl<'a> ops::Deref for Args<'a> {
    type Target = ArgMatches<'a>;

    fn deref(&self) -> &Self::Target {
        &self.matches
    }
}


//------------ File ----------------------------------------------------------

#[derive(Default)]
struct File {
    values: toml::value::Table,
    dir: PathBuf,
}

impl File {
    fn read(path: &Path) -> Self {
        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(err) => {
                println!(
                    "Cannot open config file {}: {}",
                    path.display(), err
                );
                process::exit(1);
            }
        };
        // I think unwrap here is fine if opening the file succeeded -- there
        // will always be a directory that the file lives in.
        let dir = path.parent().unwrap().into();
        let mut content = Vec::new();
        if let Err(err) = file.read_to_end(&mut content) {
            println!(
                "Failed to read config file {}: {}",
                path.display(), err
            );
            process::exit(1);
        }
        let value = match toml::de::from_slice(content.as_ref()) {
            Ok(value) => value,
            Err(err) => {
                println!(
                    "Failed to parse config file {}: {}",
                    path.display(), err
                );
                process::exit(1);
            }
        };
        match value {
            toml::Value::Table(values) => File { values, dir },
            _ => panic!("TOML file is not a table.")
        }
    }

    fn take(&mut self, key: &str) -> Option<toml::Value> {
        self.values.remove(key)
    }

    fn take_int(&mut self, key: &str) -> Option<i64> {
        match self.take(key) {
            Some(toml::Value::Integer(value)) => Some(value),
            Some(_) => {
                println!(
                    "Error in config file: '{}' expected to be an integer.",
                    key
                );
                process::exit(1);
            }
            None => None
        }
    }

    fn take_uint(&mut self, key: &str) -> Option<u64> {
        match self.take_int(key) {
            Some(val) => {
                if val < 0 {
                    println!(
                        "Error in config file: \
                         '{}' expected to be an unsigned integer.",
                        key
                    );
                    process::exit(1);
                }
                Some(val as u64)
            }
            None => None
        }
    }

    fn take_bool(&mut self, key: &str) -> Option<bool> {
        match self.take(key) {
            Some(toml::Value::Boolean(value)) => Some(value),
            Some(_) => {
                println!(
                    "Error in config file: '{}' expected to be a boolean.",
                    key
                );
                process::exit(1);
            }
            None => None
        }
    }

    fn take_str(&mut self, key: &str) -> Option<String> {
        match self.take(key) {
            Some(toml::Value::String(value)) => Some(value),
            Some(_) => {
                println!(
                    "Error in config file: '{}' expected to be a path",
                    key
                );
                process::exit(1);
            }
            None => None
        }
    }

    fn take_path(&mut self, key: &str) -> Option<PathBuf> {
        self.take_str(key).map(|path| self.dir.join(path))
    }

    fn take_str_vec<V>(&mut self, key: &str) -> Option<Vec<V>>
    where V: FromStr, V::Err: fmt::Display {
        match self.take(key) {
            Some(toml::Value::Array(array)) => {
                Some(array.iter().map(|value| {
                    let value = match value {
                        toml::Value::String(s) => s,
                        _ => {
                            println!(
                                "Error in config file: \
                                 '{}' expected to be an array of strings.",
                                key
                            );
                            process::exit(1);
                        }
                    };
                    match V::from_str(value) {
                        Ok(v) => v,
                        Err(_) => {
                            println!(
                                "Error in config file: \
                                unexpected value '{}' in '{}' field.",
                                value, key
                            );
                            process::exit(1);
                        }
                    }
                }).collect())
            }
            Some(_) => {
                println!(
                    "Error in config file: \
                     '{}' expected to be an array of strings ",
                    key
                );
                process::exit(1);
            }
            None => return None
        }
    }
    
    fn take_from_str<V>(&mut self, key: &str) -> Option<V>
    where V: FromStr, V::Err: fmt::Display {
        self.take_str(key).map(|v| {
            match V::from_str(&v) {
                Ok(v) => v,
                Err(_) => {
                    println!(
                        "Error in config file: \
                        unexpected value for '{}'.",
                        key
                    );
                    process::exit(1);
                }
            }
        })
    }

    fn take_dirs(&mut self) -> (PathBuf, PathBuf) {
        if let Some(dir) = self.take_path("base-dir") {
            (dir.join("repository"), dir.join("tals"))
        }
        else {
            (
                match self.take_path("cache-dir") {
                    Some(dir) => dir,
                    None => {
                        println!(
                            "Error in config file: \
                            missing 'cache-dir' or 'base-dir' field."
                        );
                        process::exit(1);
                    }
                },
                match self.take_path("tal-dir") {
                    Some(dir) => dir,
                    None => {
                        println!(
                            "Error in config file: \
                            missing 'tal-dir' or 'base-dir' field."
                        );
                        process::exit(1);
                    }
                },
            )
        }
    }
}


//------------ RunMode -------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunMode {
    Once,
    Repeat,
    Daemon,
}

impl RunMode {
    pub fn is_once(self) -> bool {
        self == RunMode::Once
    }

    pub fn is_daemon(self) -> bool {
        self == RunMode::Daemon
    }
}


//------------ OutputFormat --------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum OutputFormat {
    Csv,
    Json,
    Rpsl,
    None,
}


//------------ DEFAULT_TALS --------------------------------------------------

const DEFAULT_TALS: [(&str, &[u8]); 5] = [
    ("afrinic.tal", include_bytes!("../tals/afrinic.tal")),
    ("apnic.tal", include_bytes!("../tals/apnic.tal")),
    ("arin.tal", include_bytes!("../tals/arin.tal")),
    ("lacnic.tal", include_bytes!("../tals/lacnic.tal")),
    ("ripe.tal", include_bytes!("../tals/ripe.tal")),
];


//------------ The Man Page --------------------------------------------------

const MAN_PAGE: &[u8] = include_bytes!("../doc/routinator.1");

