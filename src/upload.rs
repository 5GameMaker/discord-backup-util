use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

use zip::{write::FileOptions, ZipWriter};

use crate::{config::Config, log::Logger, temp::temp_path, Defer};

pub fn upload<'a, L: Logger>(config: &'a Config, log: &'a mut L) {
    log.info("Trying to initiate a backup...");

    let mut head = config
        .webhook
        .send(|x| x.content("Starting backup process..."), log);

    let dir = Defer::new(temp_path(), |x| fs::remove_dir_all(x));
    if let Err(why) = fs::create_dir(&*dir) {
        log.error(&format!("Failed to create dir: {why}"));
        head.edit(&config.webhook, "Setup failed", log);
        return;
    }
    let script = Defer::new(temp_path(), |x| fs::remove_file(x));
    if let Err(why) = fs::write(&*script, &config.script) {
        log.error(&format!("Failed to write script file: {why}"));
        head.edit(&config.webhook, "Setup failed", log);
        return;
    }

    let mut iter = config.shell.iter();
    let mut proc = match Command::new(iter.next().unwrap())
        .args(iter)
        .arg(&*script)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(&*dir)
        .spawn()
    {
        Ok(x) => x,
        Err(why) => {
            log.error(&format!("Failed to spawn child process: {why}"));
            head.edit(&config.webhook, "Failed to start backup process", log);
            return;
        }
    };

    head.edit(&config.webhook, "Backing up data...", log);

    match proc.wait() {
        Ok(x) => {
            if !x.success() {
                log.error("Backup process failed: exited with non-zero error code");
                head.edit(&config.webhook, "Backup process failed", log);
                return;
            }
        }
        Err(why) => {
            log.error(&format!("Backup process failed: {why}"));
            head.edit(&config.webhook, "Backup process failed", log);
            return;
        }
    }

    let archive = Defer::new(temp_path(), |x| fs::remove_file(x));

    let file = match File::create(&*archive) {
        Ok(x) => x,
        Err(why) => {
            log.error(&format!("Failed to create temporary file: {why}"));
            head.edit(&config.webhook, "Failed to start backup process", log);
            return;
        }
    };
    let mut zip = ZipWriter::new(file);

    log.info("Compressing the archive...");
    head.edit(&config.webhook, "Compressing the archive...", log);

    fn walk<L: Logger>(
        path: PathBuf,
        name: String,
        zip: &mut ZipWriter<File>,
        options: FileOptions<'_, ()>,
        log: &mut L,
    ) {
        for x in match fs::read_dir(path) {
            Ok(x) => x,
            Err(why) => {
                log.warn(&format!("readdir() failed: {why}"));
                return;
            }
        } {
            let x = match x {
                Ok(x) => x,
                Err(why) => {
                    log.warn(&format!("readdir() failed: {why}"));
                    return;
                }
            };

            let metadata = match x.metadata() {
                Ok(x) => x,
                Err(why) => {
                    log.warn(&format!("metadata() failed: {why}"));
                    return;
                }
            };

            if metadata.is_file() {
                if let Err(why) = zip.start_file(
                    format!("{name}/{}", x.file_name().into_string().unwrap())
                        .trim_start_matches('/'),
                    options.large_file(metadata.len() >= 1024 * 1024 * 1024 * 4),
                ) {
                    log.warn(&format!("Failed to start zip header: {why}"));
                    return;
                }

                let mut buffer = vec![0; 8192];
                let mut file = match File::open(x.path()) {
                    Ok(x) => x,
                    Err(why) => {
                        log.warn(&format!("open() failed: {why}"));
                        return;
                    }
                };

                loop {
                    match file.read(&mut buffer) {
                        Ok(x) => {
                            if x == 0 {
                                break;
                            } else if let Err(why) = zip.write_all(&buffer[0..x]) {
                                log.warn(&format!("write() failed: {why}"));
                                return;
                            }
                        }
                        Err(why) => {
                            log.warn(&format!("read() failed: {why}"));
                            return;
                        }
                    }
                }

                log.info(
                    format!("Added file {name}/{}", x.file_name().into_string().unwrap())
                        .trim_start_matches('/'),
                );
            } else {
                walk(
                    x.path(),
                    format!("{name}/{}", x.file_name().into_string().unwrap())
                        .trim_start_matches('/')
                        .to_string(),
                    zip,
                    options,
                    log,
                );
            }
        }
    }
    let password = config.password.as_ref();
    walk(
        dir.clone(),
        String::new(),
        &mut zip,
        {
            let options = FileOptions::default()
                .compression_level(Some(config.compression_level))
                .compression_method(zip::CompressionMethod::Deflated);

            if let Some(x) = password {
                options.with_aes_encryption(zip::AesMode::Aes256, x)
            } else {
                options
            }
        },
        log,
    );

    drop(script);

    let mut file = match File::open(&*archive) {
        Ok(x) => x,
        Err(why) => {
            log.error(&format!("Failed to open temporary file: {why}"));
            head.edit(&config.webhook, "Failed to start backup process", log);
            return;
        }
    };

    if let Err(why) = zip.finish() {
        log.error(&format!("Failed to contruct a zip archive: {why}"));
        head.edit(&config.webhook, "Failed to finalize a zip archive", log);
        return;
    }

    const CHUNK_SIZE: usize = 1000 * 1000 * 25;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut chunks = 0u32;

    head.edit(&config.webhook, "Publishing artifact...", log);

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
                    log.error(&format!("Failed to upload artifact: {why}"));
                    head.edit(&config.webhook, "Upload failed", log);
                    continue;
                }
            }
        }

        if ptr == CHUNK_SIZE || end {
            head.reply(
                &config.webhook,
                move |x| x.file(format!("chunk_{chunks}.zip"), buffer[0..ptr].to_vec()),
                log,
            );
            break;
        }
    }

    head.edit(&config.webhook, format!("Backup completed successfully.\n\nTo assemble the original archive, download all {chunks} chunks and concatenate them into a single file"), log);

    println!("Backup completed successfully");
}
