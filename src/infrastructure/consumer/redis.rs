//! Redis script-command consumer (feature `cache`) — development convenience
//! that mirrors the same inbound event contract without a message broker: it
//! `BLPOP`s the list `hex:script:commands` (push with `LPUSH` from another
//! process or RedisInsight).
//!
//! Env: `HEX_REDIS_URL` (default `redis://127.0.0.1:6379`)

use std::sync::Arc;
use std::time::Duration;

use redis::Commands;

use crate::application::ScriptCommandUseCase;
use super::parse_command;

const LIST: &str = "hex:script:commands";

pub fn start(use_case: &Arc<ScriptCommandUseCase>, _topic: &str) -> bool {
    let url = std::env::var("HEX_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let client = match redis::Client::open(url) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[hex] redis consumer: {e}");
            return false;
        }
    };
    let mut conn = match client.get_connection() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[hex] redis consumer connect: {e}");
            return false;
        }
    };
    let use_case = Arc::clone(use_case);

    std::thread::spawn(move || loop {
        match conn.blpop::<_, Option<(String, String)>>(LIST, 0.0) {
            Ok(Some((_, body))) => match parse_command(body.as_bytes()) {
                Some(cmd) => {
                    let _ = use_case.handle(&cmd);
                }
                None => eprintln!("[hex] redis: dropping unparseable message"),
            },
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                eprintln!("[hex] redis blpop: {e}");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    });

    true
}