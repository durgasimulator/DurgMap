mod d2;
mod map;
mod logger;
mod json;
mod http;

use crate::d2::d2_client::D2Client;
use crate::d2::d2_data::get_act;
use crate::json::SeedData;

use clap::Parser;
use log::LevelFilter;
use std::time::Instant;
use std::path::Path;
use std::path::PathBuf;

use crate::logger::configure_logging;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// D2 Game Path
    #[arg(value_name = "GAME_PATH")]
    game_path: Option<String>,

    /// Map Seed
    #[arg(short, long)]
    seed: Option<u32>,

    /// Game Difficulty (0: Normal, 1: Nightmare, 2: Hell)
    #[arg(short, long, default_value_t = 0)]
    difficulty: u32,

    /// Dump a specific act (0-4)
    #[arg(short, long)]
    act: Option<i32>,

    /// Dump a specific map by ID
    #[arg(short, long)]
    map: Option<u32>,

    /// Save to path
    #[arg(short, long)]
    json_path: Option<String>,

    /// Increase logging level
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

/// HTTP server port (hardcoded).
const SERVER_PORT: u16 = 8000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Set up logging
    let log_level = match args.verbose {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    configure_logging(log_level);

    log::info!("d2-map starting, version: {}", VERSION);

    // Default to a folder named `game` next to the executable when no path is given.
    let game_path = match args.game_path {
        Some(path) => path,
        None => default_game_path(),
    };
    log::info!("Using game path: {}", game_path);
    let json_path: PathBuf = format_json_path(args.json_path)?;

    // D2's level generation recurses very deeply for some maps. The OS default main-thread
    // stack (~1 MB, and smaller/stricter under the 32-bit Wine build) overflows mid-generation
    // and aborts the whole process — observed as `thread 'main' has overflowed its stack` with
    // the faulting address inside a D2 DLL, followed by a crash/restart loop.
    //
    // Run ALL D2 work on one dedicated thread with a large stack. It must be a SINGLE thread:
    // the D2 game DLLs are not thread-safe and rely on per-thread state established by
    // `initialize`, so init + every request must share the same thread (as before — just with
    // a bigger stack).
    const D2_STACK_SIZE: usize = 64 * 1024 * 1024; // 64 MB reserved (committed on demand)
    let seed_arg = args.seed;
    let act_arg = args.act;
    let map_arg = args.map;
    let difficulty = args.difficulty;

    let worker = std::thread::Builder::new()
        .name("d2-map".into())
        .stack_size(D2_STACK_SIZE)
        .spawn(move || -> Result<(), String> {
            unsafe {
                let mut client = D2Client::new();

                let init_start = Instant::now();
                client.initialize(&game_path).map_err(|e| e.to_string())?;
                log::info!(
                    "Initialization complete, version: {}, duration: {}ms",
                    VERSION,
                    init_start.elapsed().as_millis()
                );

                if seed_arg.is_some() || act_arg.is_some() || map_arg.is_some() {
                    let seed = seed_arg.unwrap_or(0xff00ff00);
                    dump_maps(&mut client, seed, difficulty, act_arg, map_arg, json_path);
                    return Ok(());
                }

                // No one-shot dump requested: run as a resident HTTP map server on port 8000.
                http::serve(&mut client, SERVER_PORT).map_err(|e| e.to_string())?;
            }
            Ok(())
        })?;

    // Propagate a clean error for a worker panic or an inner failure.
    worker
        .join()
        .map_err(|_| "d2-map worker thread panicked".to_string())??;

    Ok(())
}


/// The default D2 game directory: a folder named `game` in the same directory as the
/// running executable. Falls back to `./game` relative to the current directory.
fn default_game_path() -> String {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("game").to_string_lossy().to_string()
}

fn format_json_path(json_path: Option<String>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if json_path.is_none() {
        return Ok(PathBuf::new());
    }

    let json_path_str = json_path.unwrap();
    let path = Path::new(&json_path_str);
    std::fs::create_dir_all(path)?;
    
    match path.canonicalize() {
        Ok(canonical_path) => {
            log::info!("JSON output path set to: {}", canonical_path.to_string_lossy().replace("\\\\?\\", ""));
            Ok(canonical_path)
        }
        Err(_) => {
            log::error!("Failed to create JSON output path: {:?}", json_path_str);
            Ok(path.to_path_buf())
        }
    }
}



unsafe fn dump_maps(
    client: &mut D2Client,
    seed: u32,
    difficulty: u32,
    act_id: Option<i32>,
    map_id: Option<u32>,
    json_path: PathBuf,
) {
    let total_start = Instant::now();
    let mut map_count = 0;
    let mut json_maps = vec![];

    if let Some(specific_map) = map_id {
        let start = Instant::now();
        match client.dump_map(seed, difficulty, specific_map) {
            Ok(map_data) => {
                println!("\n{}", serde_json::to_string(&map_data).unwrap());
                map_count += 1;
                let duration = start.elapsed();
                log::debug!(
                    "Map generated: seed={}, difficulty={}, mapId={}, duration={}ms",
                    seed,
                    difficulty,
                    specific_map,
                    duration.as_millis()
                );
            }
            Err(e) => {
                log::warn!("Failed to generate map {}: {}", specific_map, e);
            }
        }
    } else {
        for level_id in 0..200u32 {
            if let Some(act) = act_id {
                if get_act(level_id) != act {
                    continue;
                }
            }

            let start = Instant::now();
            match client.dump_map(seed, difficulty, level_id) {
                Ok(map_data) => {
                    if json_path.as_os_str().is_empty() {
                        println!("\n{}", serde_json::to_string(&map_data).unwrap());
                    } else {
                        json_maps.push(map_data);
                    }
                    
                    map_count += 1;
                    let duration = start.elapsed();
                    log::debug!(
                        "Map generated: seed={}, difficulty={}, actId={}, mapId={}, duration={}ms",
                        seed,
                        difficulty,
                        get_act(level_id),
                        level_id,
                        duration.as_millis()
                    );
                }
                Err(_) => {
                    // Skip levels that fail to generate
                    continue;
                }
            }
        }
        if !json_path.as_os_str().is_empty() {
            let file_path = json_path.join(format!("D2_{}_{}.json", seed, difficulty));

            let json_data = SeedData {
                seed,
                difficulty,
                levels: json_maps,
            };
            
            let json_output = serde_json::to_string_pretty(&json_data).unwrap();
            std::fs::write(&file_path, json_output).unwrap();
            log::info!("Maps saved to {}", file_path.display().to_string().replace("\\\\?\\", ""));
        }
    }

    let total_duration = total_start.elapsed();
    log::info!(
        "Map generation complete: seed={}, difficulty={}, count={}, duration={}ms",
        seed,
        difficulty,
        map_count,
        total_duration.as_millis()
    );
    
}
