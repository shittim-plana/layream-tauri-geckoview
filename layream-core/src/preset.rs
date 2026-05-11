use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

use crate::crypto;
use crate::error::LayreamError;
use crate::rpack;
use crate::types::{BotPreset, PresetEnvelope};

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
    let inner_msgpack = rmp_serde::to_vec(preset)?;
    let encrypted = crypto::encrypt(&inner_msgpack, PRESET_KEY)?;

    let envelope = PresetEnvelope {
        preset_version: 2,
        envelope_type: "preset".to_string(),
        preset: encrypted,
    };

    let outer_msgpack = rmp_serde::to_vec(&envelope)?;
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
const _RISUM_ASSET_MARKER: u8 = 1;
const _RISUM_END_MARKER: u8 = 0;

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
fn parse_risum_binary(data: &[u8]) -> Result<serde_json::Value, LayreamError> {
    // Minimum: magic(1) + version(1) + main_len(4) + at least 1 byte payload = 7
    if data.len() < 7 {
        return Err(LayreamError::Http("risum binary too short".to_string()));
    }

    // Skip magic and version (already validated by caller)
    let mut pos: usize = 2;

    let main_len = u32::from_le_bytes([
        data[pos],
        data[pos + 1],
        data[pos + 2],
        data[pos + 3],
    ]) as usize;
    pos += 4;

    if pos + main_len > data.len() {
        return Err(LayreamError::Http(format!(
            "risum main chunk length {} exceeds data size {}",
            main_len,
            data.len() - pos
        )));
    }

    let main_compressed = &data[pos..pos + main_len];
    let main_decoded = rpack::decode(main_compressed);
    let json: serde_json::Value = serde_json::from_slice(&main_decoded)?;

    // We intentionally skip asset chunks here -- parse_risum_data returns the
    // module JSON only; asset extraction is handled separately by the caller.
    Ok(json)
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
}
