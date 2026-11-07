//! Linux desktop integration: installs a launcher entry and icon so Wayland shells show the app icon.

use std::path::Path;

/// Application id; matches the `.desktop` file name and the window's app id.
pub const APP_ID: &str = "jj-dlp";

const ICON_RGBA: &[u8] = include_bytes!("../../assets/icon_128.rgba");
const ICON_SIZE: u32 = 128;

/// Writes the icon and launcher entry under `~/.local/share` when missing or out of date.
pub fn install() {
    if system_entry_exists() {
        return;
    }
    let Some(data) = directories::BaseDirs::new().map(|d| d.data_dir().to_path_buf()) else { return };
    let Ok(exe) = std::env::current_exe() else { return };
    let icon = data.join("icons/hicolor/128x128/apps").join(format!("{APP_ID}.png"));
    if !icon.is_file() {
        write_icon(&icon);
    }
    let entry = data.join("applications").join(format!("{APP_ID}.desktop"));
    write_if_changed(&entry, &desktop_entry(&exe, &icon));
}

/// True when a packaged launcher entry already exists.
fn system_entry_exists() -> bool {
    ["/usr/share/applications", "/usr/local/share/applications"].iter().any(|d| Path::new(d).join(format!("{APP_ID}.desktop")).is_file())
}

fn write_icon(path: &Path) {
    let Some(img) = image::RgbaImage::from_raw(ICON_SIZE, ICON_SIZE, ICON_RGBA.to_vec()) else { return };
    let mut png = std::io::Cursor::new(Vec::new());
    if img.write_to(&mut png, image::ImageFormat::Png).is_err() {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, png.into_inner());
}

fn write_if_changed(path: &Path, text: &str) {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

/// The launcher entry text for `exe` and the icon file.
fn desktop_entry(exe: &Path, icon: &Path) -> String {
    let quoted: String = exe
        .to_string_lossy()
        .chars()
        .flat_map(|c| match c {
            '"' | '\\' | '`' | '$' => vec!['\\', c],
            '%' => vec!['%', '%'],
            _ => vec![c],
        })
        .collect();
    format!(
        "[Desktop Entry]\nType=Application\nName=jj-dlp\nExec=\"{quoted}\"\nIcon={}\nTerminal=false\nCategories=AudioVideo;Network;\nStartupWMClass={APP_ID}\n",
        icon.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_quotes_the_exe_path() {
        let text = desktop_entry(Path::new("/opt/my app/jj-dlp"), Path::new("/i/jj-dlp.png"));
        assert!(text.contains("Exec=\"/opt/my app/jj-dlp\"\n") && text.contains("Icon=/i/jj-dlp.png\n"));
    }
}
