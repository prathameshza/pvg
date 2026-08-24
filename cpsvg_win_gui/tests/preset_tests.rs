use pvg_lib::{compile_pvg_at_time, rasterize_pvg_to_png, transpile_pvg_to_svg};
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

        // 1. Test Compilation & Evaluation
        let result = compile_pvg_at_time(&content, 1.0);
        assert!(
            result.is_ok(),
            "Preset {} failed compilation: {:?}",
            file_path,
            result.err()
        );

        let draw_list = result.unwrap();
        assert!(!draw_list.items.is_empty(), "Preset {} generated 0 primitives", file_path);

        // 2. Test SVG Transpilation
        let svg_res = transpile_pvg_to_svg(&content, 1.0);
        assert!(svg_res.is_ok(), "Preset {} failed SVG generation", file_path);
        let svg_str = svg_res.unwrap();
        assert!(svg_str.starts_with("<?xml") && svg_str.contains("<svg"), "Invalid SVG output for {}", file_path);

        // 3. Test PNG Rasterization
        let png_res = rasterize_pvg_to_png(&content, 1.0);
        assert!(png_res.is_ok(), "Preset {} failed PNG rasterization: {:?}", file_path, png_res.err());
        let png_bytes = png_res.unwrap();
        assert!(!png_bytes.is_empty(), "Preset {} produced empty PNG bytes", file_path);
        
        // Verify valid PNG Magic Header (89 50 4E 47 0D 0A 1A 0A)
        let png_header: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(
            png_bytes.len() >= 8 && &png_bytes[0..8] == &png_header,
            "Preset {} produced invalid PNG header signature",
            file_path
        );
    }
}