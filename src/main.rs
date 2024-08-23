use std::{fs::{self, File}, io::{Read, Write}, ops::{Deref, DerefMut}, path::PathBuf, process::{Command, Stdio}};

use config::parse_args;
use temp::temp_path;
use zip::{write::SimpleFileOptions, ZipWriter};

pub mod hook;
pub mod config;
pub mod temp;

struct Defer<T, G, F: Fn(&mut T) -> G>(T, F);
impl<T, G, F: Fn(&mut T) -> G> Defer<T, G, F> {
    pub fn new(value: T, fun: F) -> Self {
        Self(value, fun)
    }
}
impl<T, G, F: Fn(&mut T) -> G> Deref for Defer<T, G, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T, G, F: Fn(&mut T) -> G> DerefMut for Defer<T, G, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl<T, G, F: Fn(&mut T) -> G> Drop for Defer<T, G, F> {
    fn drop(&mut self) {
        (self.1)(&mut self.0);
    }
}
impl<I, T: AsRef<I>, G, F: Fn(&mut T) -> G> AsRef<I> for Defer<T, G, F> {
    fn as_ref(&self) -> &I {
        self.0.as_ref()
    }
}
impl<I, T: AsMut<I>, G, F: Fn(&mut T) -> G> AsMut<I> for Defer<T, G, F> {
    fn as_mut(&mut self) -> &mut I {
        self.0.as_mut()
    }
}

fn main() {
    let config = Box::leak(Box::new(parse_args()));

    let mut first = true;

    loop {
        if first {
            first = false;
        } else {
            std::thread::sleep(config.delay);
        }

        println!("Trying to initiate a backup...");

        let mut head = config.webhook.send(|x| x.content("Starting backup process..."));

        let dir = Defer::new(temp_path(), |x| fs::remove_dir_all(x));
        if let Err(why) = fs::create_dir(&*dir) {
            println!("Failed to create dir: {why}");
            head.edit(&config.webhook, "Setup failed");
            continue;
        }
        let script = Defer::new(temp_path(), |x| fs::remove_file(x));
        if let Err(why) = fs::write(&*script, &config.script) {
            println!("Failed to write script file: {why}");
            head.edit(&config.webhook, "Setup failed");
            continue;
        }

        let mut iter = config.shell.iter();
        let mut proc = match Command::new(iter.next().unwrap()).args(iter).arg(&*script).stdout(Stdio::inherit()).stderr(Stdio::inherit())
        .current_dir(&*dir).spawn() {
            Ok(x) => x,
            Err(why) => {
                println!("Failed to spawn child process: {why}");
                head.edit(&config.webhook, "Failed to start backup process");
                continue;
            }
        };

        head.edit(&config.webhook, "Backing up data...");

        match proc.wait() {
            Ok(x) => if !x.success() {
                println!("Backup process failed: exited with non-zero error code");
                head.edit(&config.webhook, "Backup process failed");
                continue;
            },
            Err(why) => {
                println!("Backup process failed: {why}");
                head.edit(&config.webhook, "Backup process failed");
                continue;
            }
        }

        let archive = Defer::new(temp_path(), |x| fs::remove_file(x));

        let file = match File::create(&*archive) {
            Ok(x) => x,
            Err(why) => {
                println!("Failed to create temporary file: {why}");
                head.edit(&config.webhook, "Failed to start backup process");
                continue;
            }
        };
        let mut zip = ZipWriter::new(file);

        head.edit(&config.webhook, "Compressing the archive...");

        fn walk(path: PathBuf, name: String, zip: &mut ZipWriter<File>, options: SimpleFileOptions) {
            for x in match fs::read_dir(path) {
                Ok(x) => x,
                Err(why) => {
                    println!("readdir() failed: {why}");
                    return;
                },
            } {
                let x = match x {
                    Ok(x) => x,
                    Err(why) => {
                        println!("readdir() failed: {why}");
                        return;
                    }
                };

                let metadata = match x.metadata() {
                    Ok(x) => x,
                    Err(why) => {
                        println!("metadata() failed: {why}");
                        return;
                    }
                };

                if metadata.is_file() {
                    if let Err(why) = zip.start_file(format!("{name}/{}", x.file_name().into_string().unwrap()).trim_start_matches('/'), options.clone()
                        .large_file(metadata.len() >= 1024 * 1024 * 1024 * 4)) {
                            println!("Failed to start zip header: {why}");
                            return;
                        }

                    let mut buffer = vec![0; 8192];
                    let mut file = match File::open(x.path()) {
                        Ok(x) => x,
                        Err(why) => {
                            println!("open() failed: {why}");
                            return;
                        }
                    };

                    loop {
                        match file.read(&mut buffer) {
                            Ok(x) => if x == 0 {
                                break;
                            } else if let Err(why) = zip.write_all(&buffer[0..x]) {
                                println!("write() failed: {why}");
                                return;
                            },
                            Err(why) => {
                                println!("read() failed: {why}");
                                return;
                            }
                        }
                    }
                } else {
                    if let Err(why) = zip.add_directory(name.clone(), SimpleFileOptions::default().compression_level(Some(10))) {
                        println!("Failed to add directory: {why}");
                        return;
                    }
                    walk(x.path(), format!("{name}/{}", x.file_name().into_string().unwrap()).trim_start_matches('/').to_string(), zip, options);
                }
            }
        }
        walk(dir.clone(), String::new(), &mut zip, {
            let options = SimpleFileOptions::default()
                .compression_level(Some(10))
                .compression_method(zip::CompressionMethod::Deflated);

            if let Some(x) = &config.password {
                options
                    .with_aes_encryption(zip::AesMode::Aes256, x)
            } else {
                options
            }
        });

        drop(script);

        let mut file = match File::open(&*archive) {
            Ok(x) => x,
            Err(why) => {
                println!("Failed to open temporary file: {why}");
                head.edit(&config.webhook, "Failed to start backup process");
                continue;
            }
        };

        if let Err(why) = zip.finish() {
            println!("Failed to contruct a zip archive: {why}");
            head.edit(&config.webhook, "Failed to finalize a zip archive");
            continue;
        }

        const CHUNK_SIZE: usize = 1000 * 1000 * 25;
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut chunks = 0u32;

        head.edit(&config.webhook, format!("Publishing artifact..."));

        loop {
            chunks += 1;
            let mut ptr = 0usize;

            let mut end = false;

            while ptr != CHUNK_SIZE {
                match file.read(&mut buffer) {
                    Ok(len) => {
                        ptr += len;
                        if len == 0 {
                            end = true;
                            break;
                        }
                    }
                    Err(why) => {
                        println!("Failed to upload artifact: {why}");
                        head.edit(&config.webhook, "Upload failed");
                        continue;
                    }
                }
            }

            if ptr == CHUNK_SIZE || end {
                head.reply(&config.webhook, move |x| x.file(format!("chunk_{chunks}.zip"), buffer[0..ptr].to_vec()));
                break;
            }
        }

        head.edit(&config.webhook, format!("Backup completed successfully.\n\nTo assemble the original archive, download all {chunks} chunks and concatenate them into a single file"));

        println!("Backup completed successfully");
    }
}
