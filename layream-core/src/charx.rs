use std::collections::HashMap;
use std::io::Read;

use flate2::read::GzDecoder;

use crate::crypto;
use crate::error::LayreamError;
use crate::types::{CharacterCardV2Risu, OldTavernChar};

#[derive(Debug, Clone)]
pub enum CardData {
    V2(CharacterCardV2Risu),
    OldTavern(OldTavernChar),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AssetInfo {
    pub name: String,
    pub size: u64,
}

#[derive(Debug)]
pub struct CharacterMetadata {
    pub card: Option<CardData>,
    pub asset_list: Vec<AssetInfo>,
    pub module_data: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct CharacterData {
    pub card: Option<CardData>,
    pub assets: HashMap<String, Vec<u8>>,
    pub module_data: Option<Vec<u8>>,
}

pub fn read_character(name: &str, data: &[u8]) -> Result<CharacterData, LayreamError> {
    let lower = name.to_lowercase();
    if lower.ends_with(".charx") || lower.ends_with(".jpeg") {
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

pub fn read_charx_metadata(data: &[u8]) -> Result<CharacterMetadata, LayreamError> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| LayreamError::Http(format!("ZIP error: {}", e)))?;

    let mut card_json: Option<String> = None;
    let mut module_data: Option<Vec<u8>> = None;
    let mut asset_list: Vec<AssetInfo> = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| LayreamError::Http(format!("ZIP entry error: {}", e)))?;

        let name = file.name().to_string();

        if name == "card.json" {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .map_err(|e| LayreamError::Http(format!("ZIP read error: {}", e)))?;
            card_json = Some(String::from_utf8_lossy(&buf).into_owned());
        } else if name == "module.risum" {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .map_err(|e| LayreamError::Http(format!("ZIP read error: {}", e)))?;
            module_data = Some(buf);
        } else if !name.ends_with(".json") && !name.ends_with('/') {
            asset_list.push(AssetInfo {
                name,
                size: file.size(),
            });
        }
    }

    let card = card_json.and_then(|json| parse_card_json(&json));

    Ok(CharacterMetadata {
        card,
        asset_list,
        module_data,
    })
}

pub fn read_charx_asset(data: &[u8], asset_name: &str) -> Result<Vec<u8>, LayreamError> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| LayreamError::Http(format!("ZIP error: {}", e)))?;

    let mut file = archive
        .by_name(asset_name)
        .map_err(|e| LayreamError::Http(format!("Asset '{}' not found: {}", asset_name, e)))?;

    let mut buf = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buf)
        .map_err(|e| LayreamError::Http(format!("ZIP read error: {}", e)))?;
    Ok(buf)
}

pub fn read_charx_asset_from_file(path: &std::path::Path, asset_name: &str) -> Result<Vec<u8>, LayreamError> {
    let file = std::fs::File::open(path)
        .map_err(|e| LayreamError::Http(format!("Open charx file: {}", e)))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| LayreamError::Http(format!("ZIP error: {}", e)))?;

    let mut entry = archive
        .by_name(asset_name)
        .map_err(|e| LayreamError::Http(format!("Asset '{}' not found: {}", asset_name, e)))?;

    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf)
        .map_err(|e| LayreamError::Http(format!("ZIP read error: {}", e)))?;
    Ok(buf)
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

    // Fallback: try stealth EXIF if no card found from tEXt chunks
    let card = if card.is_none() {
        extract_stealth_exif(data).and_then(|text| parse_card_json(&text))
    } else {
        card
    };

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

// --- Stealth EXIF (NAI steganography) ---

const SIG_ALPHA: &str = "stealth_pnginfo";
const SIG_ALPHA_COMP: &str = "stealth_pngcomp";
const SIG_RGB: &str = "stealth_rgbinfo";
const SIG_RGB_COMP: &str = "stealth_rgbcomp";

/// Decode PNG to RGBA pixels, then extract stealth payload from LSBs.
///
/// The encoding hides data in pixel LSBs using column-first traversal (x outer, y inner).
/// Protocol: signature → 32-bit payload length → payload bits → optional gzip.
pub fn extract_stealth_exif(png_data: &[u8]) -> Option<String> {
    let rgba = decode_png_to_rgba(png_data)?;
    let width = rgba.0;
    let height = rgba.1;
    let pixels = &rgba.2;

    // Try alpha-channel LSB first, then RGB LSB
    if let Some(payload) = extract_stealth_from_pixels(pixels, width, height, SteganographyMode::Alpha) {
        return Some(payload);
    }
    extract_stealth_from_pixels(pixels, width, height, SteganographyMode::Rgb)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SteganographyMode {
    Alpha,
    Rgb,
}

/// Decode PNG bytes into (width, height, RGBA pixel data).
fn decode_png_to_rgba(png_data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let decoder = png::Decoder::new(png_data);
    let mut reader = decoder.read_info().ok()?;

    let info = reader.info();
    let width = info.width as usize;
    let height = info.height as usize;
    let color_type = info.color_type;
    let bit_depth = info.bit_depth;

    if bit_depth != png::BitDepth::Eight {
        return None;
    }

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let output_info = reader.next_frame(&mut buf).ok()?;
    let raw = &buf[..output_info.buffer_size()];

    // Convert to RGBA
    let rgba = match color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => {
            let pixel_count = width * height;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for i in 0..pixel_count {
                rgba.push(raw[i * 3]);
                rgba.push(raw[i * 3 + 1]);
                rgba.push(raw[i * 3 + 2]);
                rgba.push(255);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let pixel_count = width * height;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for i in 0..pixel_count {
                let g = raw[i * 2];
                let a = raw[i * 2 + 1];
                rgba.push(g);
                rgba.push(g);
                rgba.push(g);
                rgba.push(a);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let pixel_count = width * height;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for i in 0..pixel_count {
                let g = raw[i];
                rgba.push(g);
                rgba.push(g);
                rgba.push(g);
                rgba.push(255);
            }
            rgba
        }
        // Indexed color — not supported for stealth EXIF (NAI uses RGBA)
        _ => return None,
    };

    Some((width, height, rgba))
}

/// Extract bits from pixel LSBs, detect signature, read payload length, read payload, decompress.
fn extract_stealth_from_pixels(
    pixels: &[u8],
    width: usize,
    height: usize,
    try_mode: SteganographyMode,
) -> Option<String> {
    // Phase 1: Collect enough bits to check the signature.
    // Alpha signature: SIG_ALPHA.len() * 8 bits from alpha channel LSB
    // RGB signature: SIG_RGB.len() * 8 bits from RGB channel LSBs (3 bits per pixel)

    let (sig_info, sig_comp, sig_bit_len) = match try_mode {
        SteganographyMode::Alpha => (
            SIG_ALPHA,
            SIG_ALPHA_COMP,
            SIG_ALPHA.len() * 8,
        ),
        SteganographyMode::Rgb => (
            SIG_RGB,
            SIG_RGB_COMP,
            SIG_RGB.len() * 8,
        ),
    };

    // We use a bit collector that traverses column-first (x outer, y inner)
    let mut bit_buffer: Vec<u8> = Vec::new();
    let mut pixel_iter = ColumnFirstIter::new(width, height);

    // Collect signature bits
    collect_bits_until(
        &mut bit_buffer,
        &mut pixel_iter,
        pixels,
        width,
        try_mode,
        sig_bit_len,
    )?;

    // Check signature
    let sig_string = bits_to_string(&bit_buffer[..sig_bit_len])?;
    let compressed = if sig_string == sig_info {
        false
    } else if sig_string == sig_comp {
        true
    } else {
        return None;
    };

    // Phase 2: Read 32-bit payload length
    bit_buffer.clear();
    let param_len_bits = if try_mode == SteganographyMode::Alpha {
        32
    } else {
        // RGB mode: 32 bits for param length, but since we get 3 bits per pixel,
        // we need ceil(32/3) pixels = 11 pixels = 33 bits, then discard last bit
        33
    };

    collect_bits_until(
        &mut bit_buffer,
        &mut pixel_iter,
        pixels,
        width,
        try_mode,
        param_len_bits,
    )?;

    let param_len = if try_mode == SteganographyMode::Alpha {
        bits_to_u32(&bit_buffer[..32]) as usize
    } else {
        // For RGB, we collected 33 bits. Use first 32 for the length.
        // The 33rd bit belongs to the payload.
        bits_to_u32(&bit_buffer[..32]) as usize
    };

    if param_len == 0 || param_len > 100_000_000 {
        return None;
    }

    // Phase 3: Read payload bits
    // For RGB mode, the 33rd bit from param_len phase is the first bit of payload
    let mut payload_bits: Vec<u8> = Vec::new();
    if try_mode == SteganographyMode::Rgb && bit_buffer.len() > 32 {
        payload_bits.push(bit_buffer[32]);
    }

    let remaining = param_len.saturating_sub(payload_bits.len());
    if remaining > 0 {
        collect_bits_until(
            &mut payload_bits,
            &mut pixel_iter,
            pixels,
            width,
            try_mode,
            param_len,
        )?;
    }

    // Truncate to exact param_len
    payload_bits.truncate(param_len);

    // Phase 4: Convert bits to bytes
    let bytes = bits_to_bytes(&payload_bits);

    // Phase 5: Decompress if needed, then decode as UTF-8
    if compressed {
        let mut decoder = GzDecoder::new(&bytes[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).ok()?;
        String::from_utf8(decompressed).ok()
    } else {
        String::from_utf8(bytes).ok()
    }
}

/// Column-first pixel iterator (x outer, y inner) — matches NAI steganography order.
struct ColumnFirstIter {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl ColumnFirstIter {
    fn new(width: usize, height: usize) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }
}

impl Iterator for ColumnFirstIter {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        if self.x >= self.width {
            return None;
        }
        let result = (self.x, self.y);
        self.y += 1;
        if self.y >= self.height {
            self.y = 0;
            self.x += 1;
        }
        Some(result)
    }
}

/// Collect bits from pixel LSBs into the buffer until it reaches `target_len`.
fn collect_bits_until(
    buffer: &mut Vec<u8>,
    iter: &mut ColumnFirstIter,
    pixels: &[u8],
    width: usize,
    mode: SteganographyMode,
    target_len: usize,
) -> Option<()> {
    while buffer.len() < target_len {
        let (x, y) = iter.next()?;
        let i = (y * width + x) * 4;
        if i + 3 >= pixels.len() {
            return None;
        }

        match mode {
            SteganographyMode::Alpha => {
                buffer.push(pixels[i + 3] & 1);
            }
            SteganographyMode::Rgb => {
                buffer.push(pixels[i] & 1); // R
                buffer.push(pixels[i + 1] & 1); // G
                buffer.push(pixels[i + 2] & 1); // B
            }
        }
    }
    Some(())
}

/// Convert a bit slice (each element is 0 or 1) to a UTF-8 string.
fn bits_to_string(bits: &[u8]) -> Option<String> {
    if bits.len() % 8 != 0 {
        return None;
    }
    let bytes: Vec<u8> = bits
        .chunks_exact(8)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0u8, |acc, &bit| (acc << 1) | bit)
        })
        .collect();
    String::from_utf8(bytes).ok()
}

/// Convert 32 bits to a u32 (big-endian).
fn bits_to_u32(bits: &[u8]) -> u32 {
    bits.iter()
        .take(32)
        .fold(0u32, |acc, &bit| (acc << 1) | bit as u32)
}

/// Convert a bit vector to a byte vector.
fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0u8, |acc, &bit| (acc << 1) | bit)
        })
        .collect()
}

// --- End Stealth EXIF ---

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

    /// Build a minimal RGBA PNG with stealth data embedded in alpha channel LSBs.
    /// Traversal order: column-first (x outer, y inner).
    fn build_stealth_png(payload: &str, compressed: bool) -> Vec<u8> {
        let sig = if compressed {
            SIG_ALPHA_COMP
        } else {
            SIG_ALPHA
        };

        let payload_bytes = if compressed {
            use flate2::write::GzEncoder;
            use std::io::Write;
            let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(payload.as_bytes()).unwrap();
            encoder.finish().unwrap()
        } else {
            payload.as_bytes().to_vec()
        };

        // Build bit stream: signature + 32-bit length + payload bits
        let mut bits: Vec<u8> = Vec::new();

        // Signature bits
        for &byte in sig.as_bytes() {
            for bit_idx in (0..8).rev() {
                bits.push((byte >> bit_idx) & 1);
            }
        }

        // Payload length in bits (32-bit big-endian)
        let payload_bit_len = (payload_bytes.len() * 8) as u32;
        for bit_idx in (0..32).rev() {
            bits.push(((payload_bit_len >> bit_idx) & 1) as u8);
        }

        // Payload bits
        for &byte in &payload_bytes {
            for bit_idx in (0..8).rev() {
                bits.push((byte >> bit_idx) & 1);
            }
        }

        // We need enough pixels. Column-first traversal: pixel (x,y) maps to index x*height+y.
        // For alpha mode, 1 bit per pixel.
        let total_pixels = bits.len();
        // Choose dimensions: width = ceil(total_pixels / height), height = some value
        let height = 64usize;
        let width = (total_pixels + height - 1) / height;
        // Ensure enough pixels
        let width = width.max(4);

        // Build RGBA pixels: all (128, 128, 128, 254) — alpha LSB = 0 by default
        let mut rgba = vec![0u8; width * height * 4];
        for pixel_idx in 0..(width * height) {
            let base = pixel_idx * 4;
            rgba[base] = 128;     // R
            rgba[base + 1] = 128; // G
            rgba[base + 2] = 128; // B
            rgba[base + 3] = 254; // A (LSB = 0)
        }

        // Write bits into alpha channel LSBs, column-first order
        for (bit_idx, &bit) in bits.iter().enumerate() {
            let x = bit_idx / height;
            let y = bit_idx % height;
            if x >= width {
                break;
            }
            let pixel_offset = (y * width + x) * 4;
            // Set alpha LSB
            rgba[pixel_offset + 3] = (rgba[pixel_offset + 3] & 0xFE) | bit;
        }

        // Encode as PNG using the png crate
        let mut png_buf: Vec<u8> = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_buf, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }

        png_buf
    }

    #[test]
    fn stealth_exif_uncompressed() {
        let payload = r#"{"spec":"chara_card_v2","spec_version":"2.0","data":{"name":"Stealth Girl","description":"Hidden in pixels","personality":"","scenario":"","first_mes":"Hi from stealth!","mes_example":"","creator_notes":"","system_prompt":"","post_history_instructions":"","alternate_greetings":[],"tags":[],"creator":"test","character_version":"1.0","extensions":{}}}"#;
        let png_data = build_stealth_png(payload, false);

        let extracted = extract_stealth_exif(&png_data);
        assert!(extracted.is_some(), "Should extract stealth payload");
        assert_eq!(extracted.unwrap(), payload);
    }

    #[test]
    fn stealth_exif_compressed() {
        let payload = r#"{"spec":"chara_card_v2","spec_version":"2.0","data":{"name":"Compressed Girl","description":"Gzipped stealth","personality":"","scenario":"","first_mes":"Hi from gzip!","mes_example":"","creator_notes":"","system_prompt":"","post_history_instructions":"","alternate_greetings":[],"tags":[],"creator":"test","character_version":"1.0","extensions":{}}}"#;
        let png_data = build_stealth_png(payload, true);

        let extracted = extract_stealth_exif(&png_data);
        assert!(extracted.is_some(), "Should extract compressed stealth payload");
        assert_eq!(extracted.unwrap(), payload);
    }

    #[test]
    fn stealth_exif_integration_with_read_png() {
        // A PNG with no tEXt chunks but with stealth EXIF should parse the card
        let payload = r#"{"spec":"chara_card_v2","spec_version":"2.0","data":{"name":"Stealth Card","description":"test","personality":"","scenario":"","first_mes":"Hello!","mes_example":"","creator_notes":"","system_prompt":"","post_history_instructions":"","alternate_greetings":[],"tags":[],"creator":"test","character_version":"1.0","extensions":{}}}"#;
        let png_data = build_stealth_png(payload, false);

        let result = read_character("test.png", &png_data).unwrap();
        assert!(result.card.is_some(), "Should find card via stealth EXIF fallback");
        if let Some(CardData::V2(card)) = &result.card {
            assert_eq!(card.data.name, "Stealth Card");
            assert_eq!(card.data.first_mes, "Hello!");
        } else {
            panic!("Expected V2 card data");
        }
    }

    #[test]
    fn stealth_exif_not_present() {
        // A regular PNG without stealth data should return None
        let mut png_buf: Vec<u8> = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_buf, 4, 4);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            // Random pixel data — won't match any signature
            let data = vec![128u8; 4 * 4 * 4];
            writer.write_image_data(&data).unwrap();
        }

        let extracted = extract_stealth_exif(&png_buf);
        assert!(extracted.is_none(), "Should not find stealth data in regular PNG");
    }

    #[test]
    fn bits_to_string_roundtrip() {
        let text = "stealth_pnginfo";
        let mut bits = Vec::new();
        for &byte in text.as_bytes() {
            for bit_idx in (0..8).rev() {
                bits.push((byte >> bit_idx) & 1);
            }
        }
        let result = super::bits_to_string(&bits).unwrap();
        assert_eq!(result, text);
    }

    #[test]
    fn bits_to_u32_roundtrip() {
        let value: u32 = 12345678;
        let mut bits = Vec::new();
        for bit_idx in (0..32).rev() {
            bits.push(((value >> bit_idx) & 1) as u8);
        }
        let result = super::bits_to_u32(&bits);
        assert_eq!(result, value);
    }

    #[test]
    fn v3_fields_parsed_from_json() {
        let json = r#"{
            "spec": "chara_card_v3",
            "spec_version": "3.0",
            "data": {
                "name": "V3 Character",
                "description": "test",
                "personality": "",
                "scenario": "",
                "first_mes": "Hi!",
                "mes_example": "",
                "creator_notes": "",
                "system_prompt": "",
                "post_history_instructions": "",
                "alternate_greetings": [],
                "tags": ["tag1"],
                "creator": "author",
                "character_version": "3.0",
                "extensions": {},
                "nickname": "V3Nick",
                "creation_date": 1700000000000,
                "modification_date": "2024-01-15",
                "group_only_greetings": ["Hello group!"]
            }
        }"#;

        let result = read_character("test.json", json.as_bytes()).unwrap();
        assert!(result.card.is_some());
        if let Some(CardData::V2(card)) = &result.card {
            assert_eq!(card.data.name, "V3 Character");
            assert_eq!(card.data.nickname.as_deref(), Some("V3Nick"));
            // Numeric creation_date is normalized to string
            assert_eq!(card.data.creation_date.as_deref(), Some("1700000000000"));
            assert_eq!(
                card.data.modification_date.as_deref(),
                Some("2024-01-15")
            );
            assert_eq!(
                card.data.group_only_greetings.as_deref(),
                Some(vec!["Hello group!".to_string()].as_slice())
            );
        } else {
            panic!("Expected V2 card data");
        }
    }

    #[test]
    fn v2_card_v3_fields_default_to_none() {
        let json = r#"{
            "spec": "chara_card_v2",
            "spec_version": "2.0",
            "data": {
                "name": "V2 Only",
                "description": "",
                "personality": "",
                "scenario": "",
                "first_mes": "Hello",
                "mes_example": "",
                "creator_notes": "",
                "system_prompt": "",
                "post_history_instructions": "",
                "alternate_greetings": [],
                "tags": [],
                "creator": "",
                "character_version": "1.0",
                "extensions": {}
            }
        }"#;

        let result = read_character("test.json", json.as_bytes()).unwrap();
        if let Some(CardData::V2(card)) = &result.card {
            assert!(card.data.nickname.is_none());
            assert!(card.data.creation_date.is_none());
            assert!(card.data.modification_date.is_none());
            assert!(card.data.group_only_greetings.is_none());
        } else {
            panic!("Expected V2 card data");
        }
    }

    #[test]
    fn v3_fields_roundtrip_serialization() {
        let json = r#"{
            "spec": "chara_card_v3",
            "spec_version": "3.0",
            "data": {
                "name": "Roundtrip",
                "description": "",
                "personality": "",
                "scenario": "",
                "first_mes": "",
                "mes_example": "",
                "creator_notes": "",
                "system_prompt": "",
                "post_history_instructions": "",
                "alternate_greetings": [],
                "tags": [],
                "creator": "",
                "character_version": "1.0",
                "extensions": {},
                "nickname": "Nick",
                "creation_date": "2024-01-01",
                "group_only_greetings": ["g1", "g2"]
            }
        }"#;

        let card: CharacterCardV2Risu = serde_json::from_str(json).unwrap();
        assert_eq!(card.data.nickname.as_deref(), Some("Nick"));

        // Serialize back and re-parse
        let serialized = serde_json::to_string(&card).unwrap();
        let reparsed: CharacterCardV2Risu = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reparsed.data.nickname, card.data.nickname);
        assert_eq!(reparsed.data.creation_date, card.data.creation_date);
        assert_eq!(
            reparsed.data.group_only_greetings,
            card.data.group_only_greetings
        );
    }

    #[test]
    fn v3_fields_absent_not_serialized() {
        let json = r#"{
            "spec": "chara_card_v2",
            "spec_version": "2.0",
            "data": {
                "name": "Minimal",
                "description": "",
                "personality": "",
                "scenario": "",
                "first_mes": "",
                "mes_example": "",
                "creator_notes": "",
                "system_prompt": "",
                "post_history_instructions": "",
                "alternate_greetings": [],
                "tags": [],
                "creator": "",
                "character_version": "1.0",
                "extensions": {}
            }
        }"#;

        let card: CharacterCardV2Risu = serde_json::from_str(json).unwrap();
        let serialized = serde_json::to_string(&card).unwrap();
        // None fields should not appear in output (skip_serializing_if)
        assert!(
            !serialized.contains("nickname"),
            "nickname should not appear in serialized output when None"
        );
        assert!(
            !serialized.contains("creation_date"),
            "creation_date should not appear in serialized output when None"
        );
        assert!(
            !serialized.contains("modification_date"),
            "modification_date should not appear in serialized output when None"
        );
        assert!(
            !serialized.contains("group_only_greetings"),
            "group_only_greetings should not appear in serialized output when None"
        );
    }
}
