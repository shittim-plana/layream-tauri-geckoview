use std::fs;

#[test]
fn read_real_risup_file() {
    let path = "/config/workspace/RisuExtractUtil/\u{1F964}마마젬 v1.26.11 합법_개조_preset.risup";
    let data = fs::read(path).expect("failed to read .risup file");
    let preset = layream_core::preset::read_preset("test.risup", &data)
        .expect("failed to parse .risup file");

    assert!(preset.temperature > 0.0);
    assert!(preset.max_context > 0);
    assert!(preset.max_response > 0);
}

#[test]
fn risup_roundtrip_real_file() {
    let path = "/config/workspace/RisuExtractUtil/\u{1F964}마마젬 v1.26.11 합법_개조_preset.risup";
    let data = fs::read(path).expect("failed to read .risup file");
    let preset = layream_core::preset::read_preset("test.risup", &data)
        .expect("failed to parse .risup file");

    let (exported, _) = layream_core::preset::export_preset(
        &preset,
        layream_core::preset::ExportFormat::Risup,
    )
    .expect("failed to export");

    let reimported =
        layream_core::preset::read_preset("re.risup", &exported).expect("failed to re-import");

    assert_eq!(preset.main_prompt, reimported.main_prompt);
    assert_eq!(preset.temperature, reimported.temperature);
    assert_eq!(preset.max_context, reimported.max_context);
}

#[test]
fn read_real_charx_file() {
    let path = "/config/workspace/RisuExtractUtil/제로의 사역마 최적화 버전.charx";
    let data = fs::read(path).expect("failed to read .charx file");
    let ch = layream_core::charx::read_character("test.charx", &data)
        .expect("failed to parse .charx file");

    assert!(ch.card.is_some());
    if let Some(layream_core::charx::CardData::V2(card)) = &ch.card {
        assert!(!card.data.name.is_empty());
        eprintln!("charx name: {}", card.data.name);
        eprintln!("charx assets: {}", ch.assets.len());
    }
}

#[test]
fn read_real_jpeg_charcard() {
    let path = "/config/workspace/RisuExtractUtil/미소노 미카 0.1 pre.jpeg";
    let data = fs::read(path).expect("failed to read .jpeg file");
    let ch = layream_core::charx::read_character("test.jpeg", &data)
        .expect("failed to parse .jpeg file");

    assert!(ch.card.is_some());
    if let Some(layream_core::charx::CardData::V2(card)) = &ch.card {
        assert!(!card.data.name.is_empty());
        eprintln!("jpeg card name: {}", card.data.name);
    }
}
