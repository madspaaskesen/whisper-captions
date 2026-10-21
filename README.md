# whisper-captions

[![crates.io](https://img.shields.io/crates/v/whisper-captions.svg)](https://crates.io/crates/whisper-captions)
[![docs.rs](https://docs.rs/whisper-captions/badge.svg)](https://docs.rs/whisper-captions)
[![CI](https://github.com/madspaaskesen/whisper-captions/actions/workflows/ci.yml/badge.svg)](https://github.com/madspaaskesen/whisper-captions/actions/workflows/ci.yml)

Turn [whisper.cpp](https://github.com/ggml-org/whisper.cpp) output into
**caption-ready segments with whole-word timestamps** — and SRT/VTT files.

whisper.cpp's word-level data is made of BPE tokens, not words. Join them
naively and you get captions like `M uld oon`, `was p` or `friend .`, plus
control tokens such as `[_BEG_]` and `[_TT_150]` leaking into the text.
`whisper-captions` fixes that:

```text
tokens:  "[_BEG_]"  " Mul"  "do"  "on,"  " friend"  "."
words:              "Muldoon,"        "friend."
```

- Parses every whisper.cpp output you are likely to have: `-oj`, `-ojf`
  (full JSON with tokens) and plain stdout (`[00:00:01.000 --> 00:00:03.500] …`)
- Reassembles BPE tokens into whole words with start/end times
- Drops whisper's special tokens
- Normalizes whitespace and timestamps (strings, seconds or milliseconds)
- Exports SRT and WebVTT
- No whisper dependency — works on files from any whisper.cpp version, and
  the token merger also accepts tokens from [`whisper-rs`](https://crates.io/crates/whisper-rs)

## Usage

```toml
[dependencies]
whisper-captions = "0.1"
```

```rust,no_run
// whisper-cli -m ggml-base.en.bin -f audio.wav -ojf -of out
let json = std::fs::read_to_string("out.json")?;
let segments = whisper_captions::parse_whisper_json_str(&json)?;

for segment in &segments {
    println!("{}–{} ms: {}", segment.start_ms, segment.end_ms, segment.text);
    for word in &segment.words {
        println!("    {:>6} ms  {}", word.start_ms, word.text);
    }
}

std::fs::write("out.srt", whisper_captions::to_srt(&segments))?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Only have stdout? `parse_whisper_stdout(&text)`. Have both and want the best
available? `parse_transcript(json.as_ref(), Some(&stdout))` prefers JSON and
falls back to stdout.

### With whisper-rs

Collect the tokens of a segment as `RawToken`s and let the crate merge them:

```rust
use whisper_captions::{merge_bpe_tokens, RawToken};

let tokens = vec![
    RawToken { raw_text: "[_BEG_]".into(), start_ms: 0.0, end_ms: 0.0 },
    RawToken { raw_text: " Hello".into(), start_ms: 0.0, end_ms: 400.0 },
    RawToken { raw_text: " Mul".into(), start_ms: 400.0, end_ms: 700.0 },
    RawToken { raw_text: "doon".into(), start_ms: 700.0, end_ms: 1100.0 },
];
let words = merge_bpe_tokens(tokens);
assert_eq!(words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["Hello", "Muldoon"]);
```

### CLI example

```sh
cargo run --example whisper2srt -- out.json > out.srt
cargo run --example whisper2srt -- stdout.txt --vtt > out.vtt
```

## Output

`Segment` and `Word` implement `serde::Serialize`/`Deserialize` and serialize
in camelCase, ready to hand to a frontend:

```json
{ "startMs": 0, "endMs": 2560, "durationMs": 2560, "text": "Hello Muldoon, friend.",
  "words": [{ "text": "Hello", "startMs": 0, "endMs": 400 }, …] }
```

## Origin

Extracted from [Flixlar](https://flixlar.com), a local caption editor, where
it turns whisper.cpp auto-captions into editable subtitle tracks.

## License

MIT
