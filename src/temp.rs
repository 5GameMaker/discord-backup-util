use std::{path::PathBuf, str::FromStr};

use rand::Rng;

pub fn temp_path() -> PathBuf {
    let name: String = rand::thread_rng().sample_iter(rand::distributions::Alphanumeric).map(|x| x as char).take(32).collect();

    PathBuf::from_str(&format!("/tmp/{name}")).unwrap()
}
