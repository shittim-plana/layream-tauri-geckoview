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

    eprintln!("Preset name: {:?}", preset.name);
    eprintln!("Temperature: {}", preset.temperature);
    eprintln!("Max context: {}", preset.max_context);
    eprintln!("Max response: {}", preset.max_response);
    eprintln!("AI model: {:?}", preset.ai_model);
    eprintln!(
        "Prompt template items: {}",
        preset.prompt_template.as_ref().map_or(0, |t| t.len())
    );
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
