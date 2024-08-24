use std::fmt::Write;
use std::{fs, process::exit, time::Duration};

use crate::hook::Webhook;

#[derive(Debug)]
pub struct Config {
    pub webhook: Webhook,
    pub script: String,
    pub shell: Vec<String>,
    pub delay: Duration,
    pub password: Option<String>,
    pub compression_level: i64,
}

struct TimeColumn {
    pub aliases: &'static [&'static str],
    pub time: Duration,
}
impl TimeColumn {
    pub const fn new(aliases: &'static [&'static str], time: Duration) -> Self {
        Self { aliases, time }
    }
}

const TIME_TABLE: &[TimeColumn] = &[
    TimeColumn::new(
        &[
            "ms",
            "milisecond",
            "miliseconds",
            "millisecond",
            "milliseconds",
        ],
        Duration::from_millis(1),
    ),
    TimeColumn::new(&["s", "second", "seconds"], Duration::from_secs(1)),
    TimeColumn::new(&["m", "min", "minute", "minutes"], Duration::from_secs(60)),
    TimeColumn::new(&["h", "hour", "hours"], Duration::from_secs(60 * 60)),
    TimeColumn::new(&["d", "day", "days"], Duration::from_secs(60 * 60 * 24)),
    TimeColumn::new(
        &["w", "week", "weeks"],
        Duration::from_secs(60 * 60 * 24 * 7),
    ),
    TimeColumn::new(
        &["n", "mon", "month", "months"],
        Duration::from_secs(2628288),
    ),
    TimeColumn::new(
        &["y", "year", "years"],
        Duration::from_secs(60 * 60 * 24 * 365 + 60 * 60 * 24 * 6),
    ),
];

pub fn parse_args() -> Config {
    let mut args = std::env::args();
    let exe = args.next().unwrap_or("discord-backup-util".into());
    let mut config = args.next().unwrap_or("backup_config".into());

    let mut setup = false;

    while config.starts_with("--") {
        if config == "--setup" {
            setup = true;
        }

        config = args.next().unwrap_or("backup_config".into());

        if config == "--" {
            break;
        }
    }

    if setup {
        if let Err(why) = fs::write(&config, include_str!("../backup_config")) {
            println!("{exe}: failed to write to config file {config:?}\n\n{why}");
            exit(-1);
        }
        exit(1);
    }

    let file = match fs::read_to_string(&config) {
        Ok(x) => x,
        Err(why) => {
            eprintln!("{exe}: failed to read config file {config:?}\n\n{why}");
            exit(-1);
        }
    };

    let mut lines = file.lines().peekable();

    let mut webhook = None;
    let mut delay = None;
    let mut password = None;
    let mut compression = None;

    while let Some(x) = lines.peek() {
        let x = x.trim();

        if x.starts_with("#!") {
            break;
        }

        let x = lines.next().unwrap().trim();

        if x.starts_with("#") || x.is_empty() {
            continue;
        }

        if x.starts_with("password ") {
            if password
                .replace(x.split_once(' ').unwrap().1.to_string())
                .is_some()
            {
                eprintln!("{exe}: cannot set multiple passwords");
                exit(-1);
            }
            continue;
        }

        if x.starts_with("compression ") {
            if let Ok(value) = x.split_once(' ').unwrap().1.parse::<i64>() {
            if compression 
                .replace(value)
                .is_some()
            {
                eprintln!("{exe}: cannot set multiple compression levels");
                exit(-1);
            }
            continue;
            } else {
                eprintln!("{exe}: invalid compression value");
                exit(-1);
            }
        }

        if x.starts_with("webhook ") {
            if webhook
                .replace(Webhook::new(x.split_once(' ').unwrap().1.to_string()))
                .is_some()
            {
                eprintln!("{exe}: cannot send to multiple webhooks");
                exit(-1);
            }
            continue;
        }

        if x.starts_with("every ") {
            if delay.is_some() {
                eprintln!("{exe}: cannot assign multiple days");
                exit(-1);
            }
            delay.replace(Duration::ZERO);
            let d = delay.as_mut().unwrap();
            let mut iter = x.split(' ').skip(1);
            while let Some(x) = iter.next() {
                if let Ok(value) = x.parse() {
                    let Some(unit) = iter.next() else {
                        eprintln!("{exe}: failed to parse duration: unit is not specified");
                        exit(-1);
                    };

                    let Some(unit) = TIME_TABLE.iter().find(|x| x.aliases.contains(&unit)) else {
                        eprintln!("{exe}: failed to parse duration: unknown unit '{unit}'");
                        exit(-1);
                    };

                    *d += unit.time * value;

                    continue;
                }
                if let Some(unit) = x.find::<fn(char) -> bool>(|x| !x.is_numeric()) {
                    if unit != 0 {
                        let (value, unit) = x.split_at(unit);
                        let Some(value): Option<u32> = value.parse().ok() else {
                            eprintln!("{exe}: failed to parse duration: invalid time");
                            exit(-1);
                        };

                        let Some(unit) = TIME_TABLE.iter().find(|x| x.aliases.contains(&unit))
                        else {
                            eprintln!("{exe}: failed to parse duration: unknown unit '{unit}'");
                            exit(-1);
                        };

                        *d += unit.time * value;

                        continue;
                    }
                }
                if let Some(unit) = TIME_TABLE.iter().find(|y| y.aliases.contains(&x)) {
                    *d += unit.time;
                    continue;
                }
                eprintln!("{exe}: failed to parse duration: undefined directive");
                exit(-1);
            }
            continue;
        }

        eprintln!("{exe}: failed to parse config: undefined directive");
        exit(-1);
    }

    let shellstr = match lines
        .next()
        .and_then(|x| x.trim().strip_prefix("#!"))
        .map(|x| x.trim())
        .and_then(|x| if x.is_empty() { None } else { Some(x) })
    {
        Some(x) => x,
        None => {
            eprintln!("{exe}: failed to parse config: no shell specified");
            exit(-1);
        }
    };

    let shell: Vec<String> = shellstr.split(' ').map(|x| x.to_owned()).collect();

    let script = lines.fold(String::new(), |mut acc, x| {
        writeln!(acc, "{x}").expect("Failed to write to string");
        acc
    });

    Config {
        webhook: match webhook {
            Some(x) => x,
            None => {
                eprintln!("{exe}: failed to parse config: missing webhook directive");
                exit(-1);
            }
        },
        delay: match delay {
            Some(x) => x,
            None => {
                eprintln!("{exe}: failed to parse config: missing every directive");
                exit(-1);
            }
        },
        compression_level: compression.unwrap_or(10),
        shell,
        script,
        password,
    }
}
