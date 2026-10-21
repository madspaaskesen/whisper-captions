//! Convert a whisper.cpp output file to SRT (or VTT with `--vtt`).
//!
//!     whisper-cli -m model.bin -f audio.wav -ojf -of out
//!     cargo run --example whisper2srt -- out.json > out.srt
//!     cargo run --example whisper2srt -- stdout.txt --vtt > out.vtt
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: whisper2srt <whisper.json | stdout.txt> [--vtt]");
        process::exit(2);
    };
    let input = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        process::exit(1);
    });

    let json = serde_json::from_str(&input).ok();
    let segments = whisper_captions::parse_transcript(json.as_ref(), Some(&input));

    if args.iter().any(|a| a == "--vtt") {
        print!("{}", whisper_captions::to_vtt(&segments));
    } else {
        print!("{}", whisper_captions::to_srt(&segments));
    }
}
