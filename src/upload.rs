use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::PathBuf,
    process::{Command, Stdio},
    rc::Rc,
    sync::{
        atomic::{AtomicU64, AtomicUsize},
        Mutex,
    },
};

use zip::{write::FileOptions, ZipWriter};

use crate::{
    config::Config,
    hook::{Message, Webhook},
    log::Logger,
    temp::temp_path,
    Defer,
};

fn upload_chunked(
    webhook: &Webhook,
    mut file: impl Read,
    name: impl Fn(usize) -> String,
    uploaded: impl Fn(Message, usize) -> std::io::Result<()>,
    log: &mut impl Logger,
) -> std::io::Result<usize> {
    const CHUNK_SIZE: usize = 1000 * 1000 * 25;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut i = 0;

    loop {
        let mut ptr = 0usize;

        let mut end = false;

        while ptr < CHUNK_SIZE {
            match file.read(&mut buffer[ptr..]) {
                Ok(len) => {
                    ptr += len;
                    if len == 0 {
                        end = true;
                        break;
                    }
                }
                Err(why) => {
                    log.error(&format!("Failed to upload artifact: {why}"));
                    return Err(why);
                }
            }
        }

        if ptr == CHUNK_SIZE || end {
            uploaded(
                webhook.send(|x| x.file(name(i), buffer[0..ptr].to_vec()), log),
                i,
            )?;
            if end {
                break Ok(i);
            }
        }

        i += 1;
    }
}

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

    if let Err(why) = zip.finish() {
        log.error(&format!("Failed to contruct a zip archive: {why}"));
        head.edit(&config.webhook, "Failed to finalize a zip archive", log);
        return;
    }

    drop(dir);

    let file = match File::open(&*archive) {
        Ok(x) => x,
        Err(why) => {
            log.error(&format!("Failed to open temporary file: {why}"));
            head.edit(&config.webhook, "Failed to start backup process", log);
            return;
        }
    };

    let volumes = ["B", "KiB", "MiB", "GiB", "TiB"];
    match file.metadata() {
        Ok(x) => {
            let mut volume = 0;
            let mut size = x.size() as f32;
            while size >= 1024.0 && volume < volumes.len() - 1 {
                size /= 1024.0;
                volume += 1;
            }
            log.info(&format!(
                "Final archive size: {size:0.3}{}",
                volumes[volume]
            ));
        }
        Err(why) => {
            log.error(&format!("Failed to fetch file metadata: {why}"));
            head.edit(&config.webhook, "Failed to fetch file metadata", log);
            return;
        }
    }

    head.edit(&config.webhook, "Publishing artifact...", log);

    let delete_file = |x: &mut PathBuf| drop(fs::remove_file(x).ok());

    let mut script_path = Defer::new(temp_path(), delete_file);
    let mut script_file = Rc::new(Mutex::new(
        match File::options()
            .write(true)
            .truncate(true)
            .create_new(true)
            .open(&*script_path)
        {
            Ok(x) => x,
            Err(why) => {
                log.error(&format!("Failed to create download script: {why}"));
                head.edit(&config.webhook, "Failed to create download script", log);
                return;
            }
        },
    ));

    if let Err(why) = script_file.lock().unwrap()
        .write_all(format!(r#"dl(){{ curl -f -L "$(curl -f -L "{}/messages/$1"|grep -Eo '"url":"[^"]+"'|grep -Eo 'https[^"]+')">>dl_backup.zip;if [ ! $? -eq 0 ];then sleep 5;dl "$1";fi }};printf "">dl_backup.zip"#, config.webhook.url()).as_bytes())
    {
        log.error(&format!("Failed to create download script: {why}"));
        head.edit(&config.webhook, "Failed to create download script", log);
        return;
    }

    let chunks = match upload_chunked(
        &config.webhook,
        file,
        |i| format!("chunk_{i}.zip"),
        |msg, _| {
            script_file
                .lock()
                .unwrap()
                .write_all(format!(";dl {}", msg.id.unwrap()).as_bytes())
        },
        log,
    ) {
        Ok(x) => x + 1,
        Err(why) => {
            log.error(&format!("Failed to upload artifact: {why}"));
            head.edit(&config.webhook, "Failed to upload artifact", log);
            return;
        }
    };

    head.edit(&config.webhook, "Uploading download script...", log);
    config.webhook.send(|x| x.content(":warning: Do not manually download files below! :warning:\n\nThose are for the download script."), log);

    let mut lol = 0usize;

    loop {
        if let Err(why) = script_file.lock().unwrap().flush() {
            log.error(&format!("Failed to upload download script: {why}"));
            head.edit(&config.webhook, "Failed to upload download script", log);
            return;
        }

        script_file = Rc::new(Mutex::new(match File::open(&*script_path) {
            Ok(x) => x,
            Err(why) => {
                log.error(&format!("Failed to upload download script: {why}"));
                head.edit(&config.webhook, "Failed to upload download script", log);
                return;
            }
        }));

        let overflow_path = Defer::new(temp_path(), delete_file);
        let overflow_file = Rc::new(Mutex::new(
            match File::options()
                .write(true)
                .truncate(true)
                .create_new(true)
                .open(&*overflow_path)
            {
                Ok(x) => x,
                Err(why) => {
                    log.error(&format!("Failed to upload download script: {why}"));
                    head.edit(&config.webhook, "Failed to upload download script", log);
                    return;
                }
            },
        ));

        if let Err(why) = overflow_file.lock().unwrap()
            .write_all(format!(r#"TFILE=mktemp;dl(){{ curl -f -L "$(curl -f -L "{}/messages/$1"|grep -Eo '"url":"[^"]+"'|grep -Eo 'https[^"]+')">>$TFILE;if [ ! $? -eq 0 ];then sleep 5;dl "$1";fi }};printf "">$TFILE"#, config.webhook.url()).as_bytes())
        {
            log.error(&format!("Failed to upload download script: {why}"));
            head.edit(&config.webhook, "Failed to upload download script", log);
            return;
        }

        let message_id = Rc::new(AtomicU64::default());

        match upload_chunked(
            &config.webhook,
            &mut *script_file.lock().unwrap(),
            |i| format!("script_{lol}_{i}.zip"),
            |msg, _| {
                message_id.store(msg.id.unwrap().get(), std::sync::atomic::Ordering::SeqCst);
                overflow_file
                    .lock()
                    .unwrap()
                    .write_all(format!(";dl {}", msg.id.unwrap()).as_bytes())
            },
            log,
        ) {
            Ok(0) => {
                config.webhook.send(|x| x.content(format!("Upload complete!\n\nTo automatically download the backup archive, use the following script:```sh\ncurl -f -L \"$(curl -f -L \"{}/messages/{}\" | grep -Eo '\"url\":\"[^\"]+\"' | grep -Eo 'https[^\"]+')\" | sh -\n```\n\nMake sure `curl` and `grep` are installed.", config.webhook.url(), message_id.load(std::sync::atomic::Ordering::SeqCst))), log);
                break;
            }
            Err(why) => {
                log.error(&format!("Failed to upload download script: {why}"));
                head.edit(&config.webhook, "Failed to upload download script", log);
                return;
            }
            _ => (),
        }

        if let Err(why) = overflow_file
            .lock()
            .unwrap()
            .write_all(r#";sh $TFILE;rm $TFILE"#.as_bytes())
        {
            log.error(&format!("Failed to upload download script: {why}"));
            head.edit(&config.webhook, "Failed to upload download script", log);
            return;
        }

        script_path = overflow_path;

        lol += 1;
    }

    head.edit(&config.webhook, format!("Backup completed successfully.\n\nTo assemble the original archive, download all {chunks} chunks and concatenate them into a single file"), log);

    println!("Backup completed successfully");
}
