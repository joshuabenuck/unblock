use std::path::Path;
use std::env;
use gate_build::AssetPacker;

fn main() {
    let is_wasm = env::var("TARGET").map(|t| t.starts_with("wasm32")).unwrap_or(false);
    let out_dir = env::var("OUT_DIR").unwrap();
    let gen_code_path = Path::new(&out_dir).join("asset_id.rs");

    let assets_dir = if is_wasm { "html" } else { "assets" };
    let mut packer = AssetPacker::new(Path::new(assets_dir));
    packer.cargo_rerun_if_changed();
    packer.sprites(Path::new("src_assets/sprites"));
    packer.music(Path::new("src_assets/music"));
    packer.sounds(Path::new("src_assets/sounds"));
    if is_wasm { packer.gen_javascript_and_html(); }
    packer.gen_asset_id_code(&gen_code_path);
}
