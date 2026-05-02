use std::collections::HashMap;
use std::io::Read;

use crate::crypto;
use crate::error::LayreamError;
use crate::types::{CharacterCardV2Risu, OldTavernChar};

#[derive(Debug, Clone)]
pub enum CardData {
    V2(CharacterCardV2Risu),
    OldTavern(OldTavernChar),
}

#[derive(Debug)]
pub struct CharacterData {
    pub card: Option<CardData>,
    pub assets: HashMap<String, Vec<u8>>,
    pub module_data: Option<Vec<u8>>,
}

pub fn read_character(name: &str, data: &[u8]) -> Result<CharacterData, LayreamError> {
    let lower = name.to_lowercase();
    if lower.ends_with(".charx") {
        read_charx(data)
    } else if lower.ends_with(".png") {
        read_png(data)
    } else if lower.ends_with(".json") {
        read_json(data)
    } else {
        Err(LayreamError::UnsupportedFileFormat(
            lower.rsplit('.').next().unwrap_or("unknown").to_string(),
        ))
    }
}

fn read_charx(data: &[u8]) -> Result<CharacterData, LayreamError> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| LayreamError::Http(format!("ZIP error: {}", e)))?;

    let mut card_json: Option<String> = None;
    let mut module_data: Option<Vec<u8>> = None;
    let mut assets: HashMap<String, Vec<u8>> = HashMap::new();

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| LayreamError::Http(format!("ZIP entry error: {}", e)))?;

        let name = file.name().to_string();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| LayreamError::Http(format!("ZIP read error: {}", e)))?;

        if name == "card.json" {
            card_json = Some(String::from_utf8_lossy(&buf).into_owned());
        } else if name == "module.risum" {
            module_data = Some(buf);
        } else if !name.ends_with(".json") {
            assets.insert(name, buf);
        }
    }

    let card = card_json
        .and_then(|json| parse_card_json(&json));

    Ok(CharacterData {
        card,
        assets,
        module_data,
    })
}

fn read_png(data: &[u8]) -> Result<CharacterData, LayreamError> {
    let chunks = read_png_text_chunks(data);
    let mut assets: HashMap<String, Vec<u8>> = HashMap::new();

    let mut raw_card: Option<String> = None;
    for chunk in &chunks {
        if chunk.key == "ccv3" || (chunk.key == "chara" && raw_card.is_none()) {
            raw_card = Some(chunk.value.clone());
        }
        if chunk.key.starts_with("chara-ext-asset_") {
            let asset_name = chunk.key.strip_prefix("chara-ext-asset_").unwrap_or("");
            if let Ok(decoded) = base64_decode(&chunk.value) {
                assets.insert(asset_name.to_string(), decoded);
            }
        }
    }

    let card = raw_card.and_then(|raw| {
        if raw.starts_with("rcc||") {
            parse_rcc(&raw).ok().flatten()
        } else {
            base64_decode(&raw)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .and_then(|json| parse_card_json(&json))
        }
    });

    Ok(CharacterData {
        card,
        assets,
        module_data: None,
    })
}

fn read_json(data: &[u8]) -> Result<CharacterData, LayreamError> {
    let card = String::from_utf8_lossy(data);
    let parsed = parse_card_json(&card);
    Ok(CharacterData {
        card: parsed,
        assets: HashMap::new(),
        module_data: None,
    })
}

fn parse_card_json(json: &str) -> Option<CardData> {
    if let Ok(card) = serde_json::from_str::<CharacterCardV2Risu>(json) {
        return Some(CardData::V2(card));
    }
    if let Ok(card) = serde_json::from_str::<OldTavernChar>(json) {
        return Some(CardData::OldTavern(card));
    }
    None
}

fn parse_rcc(raw: &str) -> Result<Option<CardData>, LayreamError> {
    let parts: Vec<&str> = raw.split("||").collect();
    if parts.len() != 5 || parts[1] != "rccv1" {
        return Ok(None);
    }

    let encrypted = base64_decode(parts[2])?;
    let hash_expected = parts[3];
    let hash_actual = crypto::sha256_hex(&encrypted);
    if hash_actual != hash_expected {
        return Ok(None);
    }

    let meta_json = base64_decode(parts[4])?;
    let meta: serde_json::Value =
        serde_json::from_slice(&meta_json).map_err(LayreamError::Json)?;

    let use_password = meta
        .get("usePassword")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if use_password {
        return Ok(None);
    }

    let decrypted = crypto::decrypt(&encrypted, "RISU_NONE")?;
    let json = String::from_utf8_lossy(&decrypted);
    Ok(parse_card_json(&json))
}

struct PngTextChunk {
    key: String,
    value: String,
}

fn read_png_text_chunks(data: &[u8]) -> Vec<PngTextChunk> {
    let mut chunks = Vec::new();
    if data.len() < 8 {
        return chunks;
    }
    let mut pos = 8; // skip PNG signature

    while pos + 12 <= data.len() {
        let length = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
            as usize;
        let chunk_type = &data[pos + 4..pos + 8];

        if chunk_type == b"IEND" {
            break;
        }

        if chunk_type == b"tEXt" && pos + 8 + length <= data.len() {
            let chunk_data = &data[pos + 8..pos + 8 + length];
            let null_pos = chunk_data.iter().take(80).position(|&b| b == 0);
            if let Some(np) = null_pos {
                let key = String::from_utf8_lossy(&chunk_data[..np]).into_owned();
                let value = String::from_utf8_lossy(&chunk_data[np + 1..]).into_owned();
                chunks.push(PngTextChunk { key, value });
            }
        }

        pos += 12 + length; // 4(len) + 4(type) + data + 4(crc)
    }

    chunks
}

fn base64_decode(input: &str) -> Result<Vec<u8>, LayreamError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input.trim())
        .map_err(|e| LayreamError::Http(format!("base64 decode error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_text_chunk_parsing() {
        // Minimal PNG: signature + tEXt chunk + IEND
        let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // signature

        // tEXt chunk: key="test", value="hello"
        let key = b"test\0hello";
        let len = key.len() as u32;
        png.extend_from_slice(&len.to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(key);
        png.extend_from_slice(&[0u8; 4]); // CRC placeholder

        // IEND
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0u8; 4]); // CRC

        let chunks = read_png_text_chunks(&png);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].key, "test");
        assert_eq!(chunks[0].value, "hello");
    }

    #[test]
    fn json_character_card() {
        let json = r#"{
            "spec": "chara_card_v2",
            "spec_version": "2.0",
            "data": {
                "name": "Test Character",
                "description": "A test",
                "personality": "",
                "scenario": "",
                "first_mes": "Hello!",
                "mes_example": "",
                "creator_notes": "",
                "system_prompt": "",
                "post_history_instructions": "",
                "alternate_greetings": [],
                "tags": [],
                "creator": "tester",
                "character_version": "1.0",
                "extensions": {}
            }
        }"#;

        let result = read_character("test.json", json.as_bytes()).unwrap();
        assert!(result.card.is_some());
        if let Some(CardData::V2(card)) = &result.card {
            assert_eq!(card.data.name, "Test Character");
            assert_eq!(card.data.first_mes, "Hello!");
        }
    }

    #[test]
    fn unsupported_format() {
        assert!(read_character("test.txt", b"data").is_err());
    }
}
