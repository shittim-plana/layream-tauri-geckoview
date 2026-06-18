use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

use crate::crypto;
use crate::error::LayreamError;
use crate::rpack;
use crate::types::{BotPreset, PresetEnvelope, RisuModule};

const PRESET_KEY: &str = "risupreset";

pub fn read_preset(name: &str, data: &[u8]) -> Result<BotPreset, LayreamError> {
    let ext = name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "risup" => decode_risup(data),
        "risupreset" => decode_risupreset(data),
        "json" | "preset" => decode_json(data),
        other => Err(LayreamError::UnsupportedFileFormat(other.to_string())),
    }
}

pub enum ExportFormat {
    Risup,
    Json,
}

pub fn export_preset(
    preset: &BotPreset,
    format: ExportFormat,
) -> Result<(Vec<u8>, &'static str), LayreamError> {
    match format {
        ExportFormat::Json => {
            let json = serde_json::to_vec_pretty(preset)?;
            Ok((json, "json"))
        }
        ExportFormat::Risup => encode_risup(preset),
    }
}

fn decode_risup(data: &[u8]) -> Result<BotPreset, LayreamError> {
    let unpacked = rpack::decode(data);
    decode_risupreset(&unpacked)
}

fn decode_risupreset(data: &[u8]) -> Result<BotPreset, LayreamError> {
    let decompressed = gz_decompress(data)?;
    let envelope: PresetEnvelope = rmp_serde::from_slice(&decompressed)?;

    match envelope.preset_version {
        0 | 2 => {
            if envelope.envelope_type != "preset" {
                return Err(LayreamError::InvalidPresetType(envelope.envelope_type));
            }
            let decrypted = crypto::decrypt(&envelope.preset, PRESET_KEY)?;
            msgpack_to_preset(&decrypted)
        }
        v if v >= 3 => msgpack_to_preset(&envelope.preset),
        v => Err(LayreamError::InvalidPresetVersion(v)),
    }
}

fn msgpack_to_preset(data: &[u8]) -> Result<BotPreset, LayreamError> {
    let rmpv_val: rmpv::Value = rmp_serde::from_slice(data)?;
    let json_val = rmpv_to_json(rmpv_val);
    let preset: BotPreset = serde_json::from_value(json_val)?;
    Ok(preset)
}

pub fn rmpv_to_json(val: rmpv::Value) -> serde_json::Value {
    match val {
        rmpv::Value::Nil => serde_json::Value::Null,
        rmpv::Value::Boolean(b) => serde_json::Value::Bool(b),
        rmpv::Value::Integer(i) => {
            if let Some(n) = i.as_i64() {
                serde_json::Value::Number(n.into())
            } else if let Some(n) = i.as_u64() {
                serde_json::Value::Number(n.into())
            } else {
                serde_json::Value::Null
            }
        }
        rmpv::Value::F32(f) => serde_json::Number::from_f64(f as f64)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        rmpv::Value::F64(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        rmpv::Value::String(s) => {
            serde_json::Value::String(s.into_str().unwrap_or_default())
        }
        rmpv::Value::Binary(b) => {
            if let Ok(s) = String::from_utf8(b.clone()) {
                serde_json::Value::String(s)
            } else {
                serde_json::Value::Array(b.into_iter().map(|byte| serde_json::Value::Number(byte.into())).collect())
            }
        }
        rmpv::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(rmpv_to_json).collect())
        }
        rmpv::Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        rmpv::Value::String(s) => s.into_str(),
                        rmpv::Value::Binary(b) => String::from_utf8(b).ok(),
                        _ => None,
                    };
                    key.map(|k| (k, rmpv_to_json(v)))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        rmpv::Value::Ext(_, data) => {
            serde_json::Value::Array(data.into_iter().map(|b| serde_json::Value::Number(b.into())).collect())
        }
    }
}

fn decode_json(data: &[u8]) -> Result<BotPreset, LayreamError> {
    let preset: BotPreset = serde_json::from_slice(data)?;
    Ok(preset)
}

fn encode_risup(preset: &BotPreset) -> Result<(Vec<u8>, &'static str), LayreamError> {
    let inner_msgpack = rmp_serde::to_vec_named(preset)?;
    let encrypted = crypto::encrypt(&inner_msgpack, PRESET_KEY)?;

    let envelope = PresetEnvelope {
        preset_version: 2,
        envelope_type: "preset".to_string(),
        preset: encrypted,
    };

    let outer_msgpack = rmp_serde::to_vec_named(&envelope)?;
    let compressed = gz_compress(&outer_msgpack)?;
    let packed = rpack::encode(&compressed);

    Ok((packed, "risup"))
}

fn gz_decompress(data: &[u8]) -> Result<Vec<u8>, LayreamError> {
    let mut decoder = GzDecoder::new(data);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    Ok(buf)
}

fn gz_compress(data: &[u8]) -> Result<Vec<u8>, LayreamError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    let compressed = encoder.finish()?;
    Ok(compressed)
}

/// Binary .risum format constants (matches RisuAI module-manager spec).
const RISUM_MAGIC: u8 = 111;
const RISUM_VERSION: u8 = 0;
const RISUM_ASSET_MARKER: u8 = 1;
const RISUM_END_MARKER: u8 = 0;

/// Parse a .risum module file into JSON.
///
/// .risum files come in two variants:
///   1. **Binary container**: `[magic=111][version=0][u32le main_len][rpack(json)]`
///      followed by optional asset chunks `[1][u32le len][rpack(data)]...` and `[0]` terminator.
///      The main payload after rpack-decode is UTF-8 JSON text of `{ "type": "risuModule", "module": ... }`.
///   2. **Plain JSON**: directly parseable UTF-8 JSON.
pub fn parse_risum_data(data: &[u8]) -> Result<serde_json::Value, LayreamError> {
    if data.len() >= 2 && data[0] == RISUM_MAGIC && data[1] == RISUM_VERSION {
        parse_risum_binary(data)
    } else {
        // Attempt plain JSON parse
        let json: serde_json::Value = serde_json::from_slice(data)?;
        Ok(json)
    }
}

/// Parse the binary .risum container format.
///
/// Returns a JSON value with the main module data plus an `"assets"` array.
/// Each asset is a base64-encoded string of the raw (rpack-decoded) bytes.
/// If the file has no asset chunks the `"assets"` array is empty.
fn parse_risum_binary(data: &[u8]) -> Result<serde_json::Value, LayreamError> {
    use base64::Engine as _;

    // Minimum: magic(1) + version(1) + main_len(4) + at least 1 byte payload = 7
    if data.len() < 7 {
        return Err(LayreamError::Http("risum binary too short".to_string()));
    }

    // Skip magic and version (already validated by caller)
    let mut pos: usize = 2;

    let main_len = read_u32_le(data, &mut pos)? as usize;

    if pos + main_len > data.len() {
        return Err(LayreamError::Http(format!(
            "risum main chunk length {} exceeds data size {}",
            main_len,
            data.len() - pos
        )));
    }

    let main_compressed = &data[pos..pos + main_len];
    let main_decoded = rpack::decode(main_compressed);
    let mut json: serde_json::Value = serde_json::from_slice(&main_decoded)?;
    pos += main_len;

    // Parse asset chunks: [0x01][u32le len][rpack(data)] ... [0x00]
    let mut assets: Vec<serde_json::Value> = Vec::new();
    while pos < data.len() {
        let marker = data[pos];
        pos += 1;

        if marker == RISUM_END_MARKER {
            break;
        }
        if marker != RISUM_ASSET_MARKER {
            return Err(LayreamError::Http(format!(
                "risum: unexpected chunk marker 0x{:02x} at pos {}",
                marker,
                pos - 1
            )));
        }

        let asset_len = read_u32_le(data, &mut pos)? as usize;
        if pos + asset_len > data.len() {
            return Err(LayreamError::Http(format!(
                "risum asset chunk length {} exceeds remaining data {} at pos {}",
                asset_len,
                data.len() - pos,
                pos
            )));
        }

        let asset_encoded = &data[pos..pos + asset_len];
        let asset_bytes = rpack::decode(asset_encoded);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&asset_bytes);
        assets.push(serde_json::Value::String(b64));
        pos += asset_len;
    }

    // Attach assets to the returned JSON.  If the top-level value is an object
    // we add an "assets" key directly; otherwise we wrap in an envelope.
    if let serde_json::Value::Object(ref mut map) = json {
        map.insert(
            "assets".to_string(),
            serde_json::Value::Array(assets),
        );
    } else {
        json = serde_json::json!({
            "data": json,
            "assets": assets,
        });
    }

    Ok(json)
}

/// Read a little-endian u32 from `data` at `*pos`, advancing `*pos` by 4.
fn read_u32_le(data: &[u8], pos: &mut usize) -> Result<u32, LayreamError> {
    if *pos + 4 > data.len() {
        return Err(LayreamError::Http(format!(
            "risum: unexpected end of data reading u32 at pos {}",
            *pos
        )));
    }
    let val = u32::from_le_bytes([
        data[*pos],
        data[*pos + 1],
        data[*pos + 2],
        data[*pos + 3],
    ]);
    *pos += 4;
    Ok(val)
}

/// Encode a `RisuModule` into the binary .risum container format.
///
/// Format: `[magic=111][version=0][u32le main_len][rpack(json_bytes)][0x00 terminator]`
///
/// Assets are not included in this encoding (main payload + terminator only).
pub fn encode_risum(module: &RisuModule) -> Result<Vec<u8>, LayreamError> {
    let json_bytes = serde_json::to_vec(module)?;
    let main_encoded = rpack::encode(&json_bytes);

    let mut buf = Vec::new();

    // Header
    buf.push(RISUM_MAGIC);
    buf.push(RISUM_VERSION);

    // Main payload length + rpack-encoded data
    buf.extend_from_slice(&(main_encoded.len() as u32).to_le_bytes());
    buf.extend_from_slice(&main_encoded);

    // End marker (no asset chunks)
    buf.push(RISUM_END_MARKER);

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let json = r#"{
            "mainPrompt": "test prompt",
            "jailbreak": "",
            "globalNote": "",
            "temperature": 80,
            "maxContext": 4000,
            "maxResponse": 300,
            "frequencyPenalty": 70,
            "PresensePenalty": 70,
            "formatingOrder": ["main", "description"],
            "promptPreprocess": false,
            "bias": [],
            "ooba": {
                "max_new_tokens": 180,
                "do_sample": true,
                "temperature": 0.7,
                "top_p": 0.9,
                "typical_p": 1,
                "repetition_penalty": 1.15,
                "encoder_repetition_penalty": 1,
                "top_k": 20,
                "min_length": 0,
                "no_repeat_ngram_size": 0,
                "num_beams": 1,
                "penalty_alpha": 0,
                "length_penalty": 1,
                "early_stopping": false,
                "seed": -1,
                "add_bos_token": true,
                "truncation_length": 4096,
                "ban_eos_token": false,
                "skip_special_tokens": true,
                "top_a": 0,
                "tfs": 1,
                "epsilon_cutoff": 0,
                "eta_cutoff": 0,
                "formating": {
                    "header": "",
                    "systemPrefix": "",
                    "userPrefix": "",
                    "assistantPrefix": "",
                    "seperator": "",
                    "useName": false
                }
            },
            "ainconfig": {
                "top_p": 0.7,
                "rep_pen": 1.0625,
                "top_a": 0.08,
                "rep_pen_slope": 1.7,
                "rep_pen_range": 1024,
                "typical_p": 1.0,
                "badwords": "",
                "stoptokens": "",
                "top_k": 140
            }
        }"#;

        let preset = read_preset("test.json", json.as_bytes()).unwrap();
        assert_eq!(preset.main_prompt, "test prompt");
        assert_eq!(preset.temperature, 80.0);

        let (exported, ext) = export_preset(&preset, ExportFormat::Json).unwrap();
        assert_eq!(ext, "json");

        let reimported = read_preset("re.json", &exported).unwrap();
        assert_eq!(reimported.main_prompt, preset.main_prompt);
    }

    #[test]
    fn risup_roundtrip() {
        let json = r#"{
            "mainPrompt": "risup test",
            "jailbreak": "",
            "globalNote": "",
            "temperature": 80,
            "maxContext": 4000,
            "maxResponse": 300,
            "frequencyPenalty": 70,
            "PresensePenalty": 70,
            "formatingOrder": ["main"],
            "promptPreprocess": false,
            "bias": [],
            "ooba": {
                "max_new_tokens": 180,
                "do_sample": true,
                "temperature": 0.7,
                "top_p": 0.9,
                "typical_p": 1,
                "repetition_penalty": 1.15,
                "encoder_repetition_penalty": 1,
                "top_k": 20,
                "min_length": 0,
                "no_repeat_ngram_size": 0,
                "num_beams": 1,
                "penalty_alpha": 0,
                "length_penalty": 1,
                "early_stopping": false,
                "seed": -1,
                "add_bos_token": true,
                "truncation_length": 4096,
                "ban_eos_token": false,
                "skip_special_tokens": true,
                "top_a": 0,
                "tfs": 1,
                "epsilon_cutoff": 0,
                "eta_cutoff": 0,
                "formating": {
                    "header": "",
                    "systemPrefix": "",
                    "userPrefix": "",
                    "assistantPrefix": "",
                    "seperator": "",
                    "useName": false
                }
            },
            "ainconfig": {
                "top_p": 0.7,
                "rep_pen": 1.0625,
                "top_a": 0.08,
                "rep_pen_slope": 1.7,
                "rep_pen_range": 1024,
                "typical_p": 1.0,
                "badwords": "",
                "stoptokens": "",
                "top_k": 140
            }
        }"#;

        let preset = read_preset("test.json", json.as_bytes()).unwrap();
        let (risup_data, ext) = export_preset(&preset, ExportFormat::Risup).unwrap();
        assert_eq!(ext, "risup");

        let reimported = read_preset("test.risup", &risup_data).unwrap();
        assert_eq!(reimported.main_prompt, "risup test");
    }

    #[test]
    fn unsupported_format() {
        let result = read_preset("test.txt", b"data");
        assert!(result.is_err());
    }

    // -- risum asset parsing tests -------------------------------------------

    /// Helper: build a synthetic .risum binary from a JSON main payload and
    /// a list of raw asset byte slices.
    fn build_risum_binary(main_json: &str, assets: &[&[u8]]) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header: magic + version
        buf.push(RISUM_MAGIC);
        buf.push(RISUM_VERSION);

        // Main payload: rpack-encode the JSON text
        let main_encoded = rpack::encode(main_json.as_bytes());
        buf.extend_from_slice(&(main_encoded.len() as u32).to_le_bytes());
        buf.extend_from_slice(&main_encoded);

        // Asset chunks
        for asset in assets {
            buf.push(RISUM_ASSET_MARKER);
            let asset_encoded = rpack::encode(asset);
            buf.extend_from_slice(&(asset_encoded.len() as u32).to_le_bytes());
            buf.extend_from_slice(&asset_encoded);
        }

        // End marker
        buf.push(RISUM_END_MARKER);

        buf
    }

    #[test]
    fn risum_no_assets_returns_empty_array() {
        let main_json = r#"{"type":"risuModule","module":{"name":"test"}}"#;
        let data = build_risum_binary(main_json, &[]);
        let result = parse_risum_data(&data).unwrap();

        assert_eq!(result["type"], "risuModule");
        assert_eq!(result["module"]["name"], "test");

        let assets = result["assets"].as_array().expect("assets should be an array");
        assert!(assets.is_empty(), "expected empty assets array");
    }

    #[test]
    fn risum_two_assets_parsed() {
        use base64::Engine as _;

        let main_json = r#"{"type":"risuModule","module":{"name":"with-assets"}}"#;
        let asset_a: &[u8] = b"hello asset one";
        let asset_b: &[u8] = &[0x00, 0x01, 0x02, 0xFF, 0xFE];
        let data = build_risum_binary(main_json, &[asset_a, asset_b]);

        let result = parse_risum_data(&data).unwrap();
        assert_eq!(result["module"]["name"], "with-assets");

        let assets = result["assets"].as_array().expect("assets array");
        assert_eq!(assets.len(), 2);

        // Verify round-trip: decode base64 back and compare
        let decoded_a = base64::engine::general_purpose::STANDARD
            .decode(assets[0].as_str().unwrap())
            .unwrap();
        assert_eq!(decoded_a, asset_a);

        let decoded_b = base64::engine::general_purpose::STANDARD
            .decode(assets[1].as_str().unwrap())
            .unwrap();
        assert_eq!(decoded_b, asset_b);
    }

    #[test]
    fn risum_backward_compat_no_trailing_data() {
        // A risum binary that ends right after the main payload (no marker, no
        // terminator).  Old files might look like this.
        let main_json = r#"{"type":"risuModule","module":{"name":"legacy"}}"#;
        let main_encoded = rpack::encode(main_json.as_bytes());

        let mut data = Vec::new();
        data.push(RISUM_MAGIC);
        data.push(RISUM_VERSION);
        data.extend_from_slice(&(main_encoded.len() as u32).to_le_bytes());
        data.extend_from_slice(&main_encoded);
        // No terminator byte at all

        let result = parse_risum_data(&data).unwrap();
        assert_eq!(result["module"]["name"], "legacy");
        let assets = result["assets"].as_array().expect("assets array");
        assert!(assets.is_empty());
    }

    #[test]
    fn risum_plain_json_still_works() {
        let json = r#"{"type":"risuModule","module":{"name":"plain"}}"#;
        let result = parse_risum_data(json.as_bytes()).unwrap();
        assert_eq!(result["module"]["name"], "plain");
        // Plain JSON path does not add assets field -- that's fine, it's only
        // the binary container that carries assets.
    }

    #[test]
    fn risum_bad_marker_is_error() {
        let main_json = r#"{"type":"risuModule","module":{}}"#;
        let main_encoded = rpack::encode(main_json.as_bytes());

        let mut data = Vec::new();
        data.push(RISUM_MAGIC);
        data.push(RISUM_VERSION);
        data.extend_from_slice(&(main_encoded.len() as u32).to_le_bytes());
        data.extend_from_slice(&main_encoded);
        data.push(0x42); // unexpected marker

        let result = parse_risum_data(&data);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("unexpected chunk marker"),
            "error should mention bad marker, got: {err_msg}"
        );
    }
}
