use std::env;
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};

use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
use image::imageops::FilterType;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=images.jpg");

    if env::var("CARGO_CFG_WINDOWS").is_ok() {
        let icon_path = write_windows_icon("images.jpg")?;
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(icon_path.to_string_lossy().as_ref());
        resource.compile()?;
    }

    Ok(())
}

fn write_windows_icon(source_path: impl AsRef<Path>) -> Result<PathBuf, Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let icon_path = out_dir.join("helium-updater.ico");
    let source = image::open(source_path)?.into_rgba8();

    let mut icon_dir = IconDir::new(ResourceType::Icon);
    for size in [16, 20, 24, 32, 40, 48, 64, 128, 256] {
        let resized = image::imageops::resize(&source, size, size, FilterType::Lanczos3);
        let image = IconImage::from_rgba_data(size, size, resized.into_raw());
        icon_dir.add_entry(IconDirEntry::encode(&image)?);
    }

    let mut file = File::create(&icon_path)?;
    icon_dir.write(&mut file)?;
    Ok(icon_path)
}
