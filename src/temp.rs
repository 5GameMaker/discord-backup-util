use std::{path::PathBuf, str::FromStr};

use rand::Rng;

#[cfg(windows)]
pub fn temp_path() -> PathBuf {
    let Ok(root) = std::env::var("TEMP") else {
        panic!("'TEMP' is not set");
    };

    let name: String = rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .map(|x| x as char)
        .take(32)
        .collect();

    PathBuf::from_str(&format!("{root}\\discord-backup-util.{name}")).unwrap()
}

#[cfg(unix)]
pub fn temp_path() -> PathBuf {
    let temp = match std::env::var("TMPDIR") {
        Ok(x) => x,
        Err(_) => "/var/tmp".to_string(),
    };

    let name: String = rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .map(|x| x as char)
        .take(32)
        .collect();

    PathBuf::from_str(&format!("{temp}/discord-backup-util.{name}")).unwrap()
}
