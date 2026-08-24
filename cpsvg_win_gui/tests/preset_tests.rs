use pvg_lib::compile_pvg_at_time;
use std::fs;

#[test]
fn test_all_preset_files() {
    let files = [
        "presets/radar.pvg",
        "presets/dial.pvg",
        "presets/grid.pvg",
        "presets/spiral.pvg",
        "presets/paths.pvg",
        "presets/gears.pvg",
    ];

    for file_path in files {
        let content = fs::read_to_string(file_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", file_path, e));
        
        let result = compile_pvg_at_time(&content, 1.0);
        assert!(
            result.is_ok(),
            "Preset {} failed compilation: {:?}",
            file_path,
            result.err()
        );

        let draw_list = result.unwrap();
        assert!(!draw_list.items.is_empty(), "Preset {} generated 0 primitives", file_path);
    }
}