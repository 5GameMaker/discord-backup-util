use std::{num::NonZeroU64, ops::Add, time::Duration};

use rand::Rng;
use tinyjson::JsonValue;

use crate::log::Logger;

#[derive(Default)]
pub struct MessageBuilder(Message);
impl MessageBuilder {
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.0.content.replace(content.into());
        self
    }
    pub fn file(mut self, name: impl Into<String>, file: Vec<u8>) -> Self {
        self.0.files.push((name.into(), file));
        self
    }
}

#[derive(Default)]
pub struct Message {
    pub id: Option<NonZeroU64>,
    pub content: Option<String>,
    pub files: Vec<(String, Vec<u8>)>,
}
impl Message {
    pub fn edit<L: Logger>(&mut self, hook: &Webhook, text: impl Into<String>, logger: &mut L) {
        let text: String = text.into();

        let Some(id) = self.id else {
            panic!("Editing a message that was never sent");
        };

        let body = format!("{{\"content\":{text:?}}}");
        self.content.replace(text);

        #[allow(clippy::blocks_in_conditions)]
        loop {
            if {
                #[cfg(feature = "minreq")]
                {
                    let body = body.clone();
                    minreq::patch(format!("{}/messages/{id}", hook.0))
                        .with_header("Content-Type", "application/json")
                        .with_header("Content-Length", body.len().to_string())
                        .with_body(body)
                        .send()
                        .is_ok()
                }
                #[cfg(feature = "ureq")]
                {
                    ureq::patch(&format!("{}/messages/{id}", hook.0))
                        .set("Content-Type", "application/json")
                        .set("Content-Length", &body.len().to_string())
                        .send_string(&body)
                        .is_ok()
                }
            } {
                break;
            }

            logger.info("Failed to edit message, retrying in 10 seconds..");
            std::thread::sleep(Duration::from_secs(10));
        }
    }
}

struct ApiMessage {
    id: String,
}

#[derive(Debug)]
pub struct Webhook(String);
impl Webhook {
    pub fn new(url: String) -> Self {
        Self(url)
    }

    pub fn url(&self) -> &str {
        &self.0
    }

    /// Send a message.
    ///
    /// Will try indefinitely until success.
    pub fn send<L: Logger>(
        &self,
        message: impl Fn(MessageBuilder) -> MessageBuilder,
        logger: &mut L,
    ) -> Message {
        let mut message: Message = message(Default::default()).0;

        let mut bodies: Vec<Vec<u8>> = vec![];

        if let Some(x) = &message.content {
            bodies.push(format!("Content-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n{{\"content\":{x:?}}}").into_bytes());
        }

        for (i, (name, bytes)) in message.files.iter().enumerate() {
            let header = format!("Content-Disposition: form-data; name=\"files[{i}]\"; filename={name:?}\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
            let mut body = vec![0u8; header.len() + bytes.len()];
            body[0..header.len()].copy_from_slice(&header);
            body[header.len()..].copy_from_slice(bytes);
            bodies.push(body);
        }

        let boundary = loop {
            let boundary: String = rand::thread_rng()
                .sample_iter(rand::distributions::Alphanumeric)
                .map(|x| x as char)
                .take(32)
                .collect();

            if !bodies
                .iter()
                .any(|x| x.windows(boundary.len()).any(|x| x == boundary.as_bytes()))
            {
                break boundary;
            }
        };

        let mut body = vec![
            0;
            boundary.len().add(6) * bodies.len().add(1)
                + bodies.iter().map(|x| x.len()).sum::<usize>()
                - 2
        ];
        let mut ptr = 0usize;

        for (first, bytes) in bodies.into_iter().enumerate().map(|(i, x)| (i == 0, x)) {
            let header =
                format!("{}--{boundary}\r\n", if first { "" } else { "\r\n" }).into_bytes();
            body[ptr..ptr + header.len()].copy_from_slice(&header);
            ptr += header.len();
            body[ptr..ptr + bytes.len()].copy_from_slice(&bytes);
            ptr += bytes.len();
        }
        {
            let header = format!("\r\n--{boundary}--").into_bytes();
            body[ptr..ptr + header.len()].copy_from_slice(&header);
        }

        #[allow(clippy::blocks_in_conditions)]
        loop {
            match {
                #[cfg(feature = "minreq")]
                {
                    let body = body.clone();
                    minreq::post(format!("{}?wait=true", self.0))
                        .with_header(
                            "Content-Type",
                            format!("multipart/form-data; boundary={boundary}"),
                        )
                        .with_header("Content-Length", body.len().to_string())
                        .with_body(body)
                        .send()
                }
                #[cfg(feature = "ureq")]
                {
                    ureq::post(&format!("{}?wait=true", self.0))
                        .set(
                            "Content-Type",
                            &format!("multipart/form-data; boundary={boundary}"),
                        )
                        .set("Content-Length", &body.len().to_string())
                        .send_bytes(&body)
                }
            } {
                Ok(x) => {
                    let parsed: ApiMessage = match {
                        #[cfg(feature = "ureq")]
                        {
                            x.into_string()
                        }
                        #[cfg(feature = "minreq")]
                        {
                            x.as_str()
                        }
                    } {
                        Ok(x) => match x.parse::<JsonValue>() {
                            Ok(x) => match x {
                                JsonValue::Object(x) => match x.get("id") {
                                    Some(JsonValue::String(x)) => ApiMessage { id: x.to_owned() },
                                    x => {
                                        logger.error(&format!("Received invalid json, retrying in 5 minutes\n\nExpected string, found {x:?}"));
                                        std::thread::sleep(Duration::from_secs(300));
                                        continue;
                                    }
                                },
                                x => {
                                    logger.error(&format!("Received invalid json, retrying in 5 minutes\n\nExpected object, found {x:?}"));
                                    std::thread::sleep(Duration::from_secs(300));
                                    continue;
                                }
                            },
                            Err(why) => {
                                logger.error(&format!(
                                    "Failed to parse json, retrying in 5 minutes...\n\n{why}"
                                ));
                                std::thread::sleep(Duration::from_secs(300));
                                continue;
                            }
                        },
                        Err(why) => {
                            logger.error(&format!(
                                "Failed to parse message, retrying in 5 minutes...\n\n{why}"
                            ));
                            std::thread::sleep(Duration::from_secs(300));
                            continue;
                        }
                    };
                    message
                        .id
                        .replace(parsed.id.parse().expect("Failed to parse a number"));
                    break message;
                }
                Err(why) => {
                    logger.error(&format!(
                        "Error sending request: {why}, retrying in 1 minute..."
                    ));
                    std::thread::sleep(Duration::from_secs(60));
                }
            }
        }
    }
}
