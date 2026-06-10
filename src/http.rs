//! Resident HTTP map server for d2-map.
//!
//! Routes (compact JSON):
//!   GET /{seed}/{difficulty}          -> SeedData  (every level for the seed)
//!   GET /{seed}/{difficulty}/{mapid}  -> LevelData (a single level)
//!
//! The D2 game DLLs are NOT thread-safe, so requests are handled sequentially on a
//! single thread via `incoming_requests()`. A shared, already-initialized `D2Client`
//! is reused across all requests; acts are loaded once per act per request and then
//! unloaded to keep memory flat over a long-lived server.

use std::time::Instant;

use tiny_http::{Header, Response, Server};

use crate::d2::d2_client::D2Client;
use crate::d2::d2_data::get_act;
use crate::json::SeedData;

/// Run the HTTP server forever. `client` must already be initialized.
pub unsafe fn serve(client: &mut D2Client, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("0.0.0.0:{}", port);
    let server = Server::http(&addr).map_err(|e| format!("Failed to start HTTP server: {}", e))?;
    log::info!("d2-map HTTP server listening on http://{}", addr);
    log::info!("Routes: GET /{{seed}}/{{difficulty}}  and  GET /{{seed}}/{{difficulty}}/{{mapid}}");

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let start = Instant::now();

        let body = match parse_path(&url) {
            Some((seed, difficulty, None)) => {
                let seed_data = dump_all_levels(client, seed, difficulty);
                log::info!(
                    "GET {} -> {} levels in {}ms",
                    url,
                    seed_data.levels.len(),
                    start.elapsed().as_millis()
                );
                serde_json::to_string(&seed_data)
                    .unwrap_or_else(|_| error_json("serialization failed"))
            }
            Some((seed, difficulty, Some(mapid))) => match client.dump_map(seed, difficulty, mapid) {
                Ok(level) => {
                    log::info!(
                        "GET {} -> level {} in {}ms",
                        url,
                        mapid,
                        start.elapsed().as_millis()
                    );
                    serde_json::to_string(&level)
                        .unwrap_or_else(|_| error_json("serialization failed"))
                }
                Err(e) => {
                    log::warn!("GET {} -> error: {}", url, e);
                    error_json(&e)
                }
            },
            None => error_json(
                "Invalid parameters! Use /{seed}/{difficulty} or /{seed}/{difficulty}/{mapid}",
            ),
        };

        let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
            .expect("valid header");
        let response = Response::from_string(body).with_header(header);
        if let Err(e) = request.respond(response) {
            log::warn!("Failed to send HTTP response: {}", e);
        }
    }

    Ok(())
}

/// Dump every level for a seed, loading each act exactly once and unloading it afterwards.
unsafe fn dump_all_levels(client: &mut D2Client, seed: u32, difficulty: u32) -> SeedData {
    let mut levels = Vec::new();

    for act_id in 0..5 {
        let p_act = client.load_act(act_id, seed, difficulty);
        if p_act.is_null() {
            log::warn!("Failed to load act {} for seed {}", act_id, seed);
            continue;
        }

        for level_id in 0..200u32 {
            if get_act(level_id) != act_id {
                continue;
            }
            if let Ok(level) = client.dump_map_with_act(p_act, level_id) {
                levels.push(level);
            }
        }

        client.unload_act(p_act);
    }

    SeedData {
        seed,
        difficulty,
        levels,
    }
}

/// Parse `/{seed}/{difficulty}` or `/{seed}/{difficulty}/{mapid}` (ignoring any query string).
fn parse_path(url: &str) -> Option<(u32, u32, Option<u32>)> {
    let path = url.split('?').next().unwrap_or(url);
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match parts.as_slice() {
        [seed, difficulty] => Some((seed.parse().ok()?, difficulty.parse().ok()?, None)),
        [seed, difficulty, mapid] => Some((
            seed.parse().ok()?,
            difficulty.parse().ok()?,
            Some(mapid.parse().ok()?),
        )),
        _ => None,
    }
}

fn error_json(msg: &str) -> String {
    format!("{{\"error\":\"{}\"}}", msg.replace('"', "'"))
}
