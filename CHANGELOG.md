# Changelog

## 0.1.1

- Drop whisper special tokens (`[_BEG_]`, `[_TT_150]`, `<|endoftext|>`, …) when
  building words; new `is_special_token()`
- `to_srt()` and `to_vtt()` subtitle export
- `Segment` / `Word` implement `Deserialize` (and `Eq`)
- README, docs, `whisper2srt` example, crates.io metadata

## 0.1.0

- Initial release: parse whisper.cpp JSON (`-oj`, `-ojf`) and stdout into
  segments; merge BPE tokens into whole words with timings
