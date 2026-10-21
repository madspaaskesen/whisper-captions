use super::*;
use serde_json::json;

fn texts(segments: &[Segment]) -> Vec<&str> {
    segments.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn timestamps() {
    assert_eq!(parse_timestamp_ms(&json!("00:00:01,500")), Some(1500.0));
    assert_eq!(parse_timestamp_ms(&json!("01:02:03.4")), Some(3_723_400.0));
    assert_eq!(parse_timestamp_ms(&json!("02:03")), Some(123_000.0));
    assert_eq!(parse_timestamp_ms(&json!(" 00:00:02.25 ")), Some(2250.0));
    assert_eq!(parse_timestamp_ms(&json!(1.5)), Some(1500.0));
    assert_eq!(parse_timestamp_ms(&json!(200000)), Some(200000.0));
    assert_eq!(parse_timestamp_ms(&json!("nope")), None);
    assert_eq!(parse_timestamp_ms(&json!(null)), None);
}

#[test]
fn js_rounding_matches_math_round() {
    assert_eq!(js::js_round(2.5), 3.0);
    assert_eq!(js::js_round(-2.5), -2.0);
    assert_eq!(js::js_round(0.49999999999999994), 0.0);
}

#[test]
fn stdout_lines() {
    let stdout = "whisper_init...\n[00:00:00.000 --> 00:00:02.000]   Hello\u{3000} world \n[00:00:02.000 --> 00:00:02.000]  zero length\n[00:00:03,000 --> 00:00:04,500] Bye\n";
    let segments = parse_whisper_stdout(stdout);
    assert_eq!(texts(&segments), ["Hello world", "Bye"]);
    assert_eq!(segments[1].start_ms, 3000);
    assert_eq!(segments[1].duration_ms, 1500);
    assert!(segments[0].words.is_empty());
}

#[test]
fn whisper_cpp_full_json_merges_bpe_tokens() {
    let json = json!({
        "result": { "language": "en" },
        "transcription": [{
            "timestamps": { "from": "00:00:00,000", "to": "00:00:02,560" },
            "offsets": { "from": 0, "to": 2560 },
            "text": " Hello Muldoon, friend.",
            "tokens": [
                { "text": "[_BEG_]", "offsets": { "from": 0, "to": 0 } },
                { "text": " Hello", "offsets": { "from": 0, "to": 400 } },
                { "text": " Mul", "offsets": { "from": 400, "to": 700 } },
                { "text": "do", "offsets": { "from": 700, "to": 900 } },
                { "text": "on,", "offsets": { "from": 900, "to": 1200 } },
                { "text": " friend", "offsets": { "from": 1200, "to": 2000 } },
                { "text": ".", "offsets": { "from": 2000, "to": 2100 } }
            ]
        }]
    });
    let segments = parse_whisper_json(&json);
    assert_eq!(segments.len(), 1);
    let s = &segments[0];
    assert_eq!((s.start_ms, s.end_ms, s.duration_ms), (0, 2560, 2560));
    assert_eq!(s.text, "Hello Muldoon, friend.");
    let words: Vec<_> = s.words.iter().map(|w| (w.text.as_str(), w.start_ms, w.end_ms)).collect();
    assert_eq!(words, [("Hello", 0, 400), ("Muldoon,", 400, 1200), ("friend.", 1200, 2100)]);
}

#[test]
fn segment_time_falls_back_to_words_and_text_to_word_join() {
    let json = json!({ "segments": [
        { "words": [ { "word": " Hi", "start": 1.0, "end": 1.4 }, { "word": " there", "start": 1.4, "end": 2.0 } ] },
        { "start": 5, "end": 4, "text": "backwards" },
        { "start": "00:00:06.000", "end": "00:00:07.000", "text": "   " }
    ]});
    let segments = parse_whisper_json(&json);
    assert_eq!(texts(&segments), ["Hi there"]);
    assert_eq!((segments[0].start_ms, segments[0].end_ms), (1000, 2000));
}

#[test]
fn transcript_prefers_json_then_stdout() {
    let stdout = "[00:00:00.000 --> 00:00:01.000] from stdout";
    assert_eq!(texts(&parse_transcript(Some(&json!(null)), Some(stdout))), ["from stdout"]);
    assert_eq!(texts(&parse_transcript(Some(&json!({ "segments": [] })), Some(stdout))), ["from stdout"]);
    let json = json!({ "segments": [{ "start": 0, "end": 1, "text": "from json" }] });
    assert_eq!(texts(&parse_transcript(Some(&json), Some(stdout))), ["from json"]);
}

#[test]
fn serializes_like_the_js_objects() {
    let segments = parse_whisper_stdout("[00:00:00.000 --> 00:00:01.000] hi");
    assert_eq!(
        serde_json::to_string(&segments).unwrap(),
        r#"[{"startMs":0,"endMs":1000,"durationMs":1000,"text":"hi"}]"#
    );
}
