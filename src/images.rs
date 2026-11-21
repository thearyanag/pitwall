//! Clipboard images are normalized into private draft PNGs; only previews are scaled.
use crate::board::Config;
use image::{DynamicImage, ImageFormat, ImageReader, RgbaImage};
use ratatui::{layout::Rect, Frame};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    StatefulImage,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Cursor, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_IMAGES: usize = 10;
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;
const MAX_PIXELS: u64 = 40_000_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalImage {
    pub path: PathBuf,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub size: u64,
}
pub struct Preview {
    pub state: StatefulProtocol,
    pub title: String,
    native: Option<KittyImage>,
    pub frame_area: Option<Rect>,
}
struct KittyImage {
    serial: u64,
    payload: String,
    width: u32,
    height: u32,
    font: (u16, u16),
}
static IMAGE_SERIAL: AtomicU64 = AtomicU64::new(1);
impl Preview {
    pub fn load(path: &Path, title: String, picker: &Picker) -> Result<Self, String> {
        let decoded = decode(path)?;
        // Keep original files untouched; cap the display copy to avoid slow redraws.
        let thumb = decoded.thumbnail(1600, 1200);
        // Kitty support does not guarantee virtual/Unicode-placeholder placement works
        // through a multiplexer. Use ordinary placements, emitted AFTER the TUI diff.
        let native = if picker.protocol_type() == ProtocolType::Kitty {
            let mut png = Cursor::new(Vec::new());
            thumb
                .write_to(&mut png, ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            Some(KittyImage {
                serial: IMAGE_SERIAL.fetch_add(1, Ordering::Relaxed),
                payload: base64_simd::STANDARD.encode_to_string(png.into_inner()),
                width: thumb.width(),
                height: thumb.height(),
                font: (picker.font_size().width, picker.font_size().height),
            })
        } else {
            None
        };
        Ok(Self {
            state: picker.new_resize_protocol(thumb),
            title,
            native,
            frame_area: None,
        })
    }
    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        if let Some(image) = &self.native {
            self.frame_area = fit_area(area, image.width, image.height, image.font);
        } else {
            f.render_stateful_widget(StatefulImage::default(), area, &mut self.state);
        }
    }
}

fn fit_area(area: Rect, width: u32, height: u32, font: (u16, u16)) -> Option<Rect> {
    if area.width == 0 || area.height == 0 || width == 0 || height == 0 {
        return None;
    }
    let fx = f64::from(font.0.max(1));
    let fy = f64::from(font.1.max(1));
    let scale = (f64::from(area.width) * fx / f64::from(width))
        .min(f64::from(area.height) * fy / f64::from(height))
        .min(1.0);
    let cols = ((f64::from(width) * scale / fx).ceil() as u16)
        .max(1)
        .min(area.width);
    let rows = ((f64::from(height) * scale / fy).ceil() as u16)
        .max(1)
        .min(area.height);
    Some(Rect::new(
        area.x + (area.width - cols) / 2,
        area.y + (area.height - rows) / 2,
        cols,
        rows,
    ))
}

/// Owns only this process's visible Kitty placements. It never deletes other images.
pub struct KittyRenderer {
    id: u32,
    next_id: u32,
    shown: std::collections::HashMap<u64, (Rect, u32)>,
}
impl Default for KittyRenderer {
    fn default() -> Self {
        Self {
            id: 0x5300_0000 | (std::process::id() & 0x00ff_ffff),
            next_id: 0,
            shown: std::collections::HashMap::new(),
        }
    }
}
impl KittyRenderer {
    pub fn render_many(&mut self, previews: &[&Preview]) -> io::Result<()> {
        self.render_many_to(previews, &mut io::stdout().lock())
    }
    #[cfg(test)]
    fn render_to(&mut self, preview: Option<&Preview>, out: &mut impl Write) -> io::Result<()> {
        self.render_many_to(&preview.into_iter().collect::<Vec<_>>(), out)
    }
    fn render_many_to(&mut self, previews: &[&Preview], out: &mut impl Write) -> io::Result<()> {
        let visible = previews
            .iter()
            .filter_map(|p| p.native.as_ref().zip(p.frame_area))
            .collect::<Vec<_>>();
        let removed = self
            .shown
            .iter()
            .filter(|(serial, _)| !visible.iter().any(|(image, _)| image.serial == **serial))
            .map(|(serial, (_, id))| (*serial, *id))
            .collect::<Vec<_>>();
        for (serial, id) in removed {
            write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\")?;
            self.shown.remove(&serial);
        }
        for (image, area) in visible {
            let id = match self.shown.get(&image.serial).copied() {
                Some((previous, _)) if previous == area => continue,
                Some((_, id)) => {
                    write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\")?;
                    id
                }
                None => {
                    let id = self.id.wrapping_add(self.next_id);
                    self.next_id += 1;
                    id
                }
            };
            write!(out, "\x1b7\x1b[{};{}H", area.y + 1, area.x + 1)?;
            let chunks = image.payload.as_bytes().chunks(4096);
            let count = chunks.len();
            for (index, chunk) in chunks.enumerate() {
                write!(out, "\x1b_G")?;
                if index == 0 {
                    write!(
                        out,
                        "a=T,f=100,t=d,i={id},p=1,c={},r={},C=1,z=1,q=2,",
                        area.width, area.height
                    )?;
                }
                write!(out, "m={};", u8::from(index + 1 < count))?;
                out.write_all(chunk)?;
                out.write_all(b"\x1b\\")?;
            }
            out.write_all(b"\x1b8")?;
            self.shown.insert(image.serial, (area, id));
        }
        out.flush()
    }
    fn clear_to(&mut self, out: &mut impl Write) -> io::Result<()> {
        for (_, id) in self.shown.values() {
            write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\")?;
        }
        self.shown.clear();
        Ok(())
    }
}
impl Drop for KittyRenderer {
    fn drop(&mut self) {
        let mut out = io::stdout().lock();
        let _ = self.clear_to(&mut out);
        let _ = out.flush();
    }
}
pub fn decode(path: &Path) -> Result<DynamicImage, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("Images must be 20 MB or smaller.".into());
    }
    let reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
        return Err("Use a PNG or JPEG image.".into());
    }
    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("Image exceeds the 40-megapixel limit.".into());
    }
    ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())
}
pub fn stage(
    config: &Config,
    key: &str,
    image: DynamicImage,
    name: String,
) -> Result<LocalImage, String> {
    if !key
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Invalid draft key.".into());
    }
    if u64::from(image.width()) * u64::from(image.height()) > MAX_PIXELS {
        return Err("Image exceeds the 40-megapixel limit.".into());
    }
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let bytes = png.into_inner();
    if bytes.len() as u64 > MAX_BYTES {
        return Err("PNG is larger than 20 MB.".into());
    }
    let folder = draft_dir(config, key);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = folder.join(format!("image-{unique:x}.png"));
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(LocalImage {
        path,
        name,
        width: image.width(),
        height: image.height(),
        size: bytes.len() as u64,
    })
}
pub fn draft_dir(config: &Config, key: &str) -> PathBuf {
    config.state_dir().join("images").join(key)
}
pub fn cleanup(config: &Config, key: &str) {
    if key.starts_with("sprout-")
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        let _ = fs::remove_dir_all(draft_dir(config, key));
    }
}
pub enum ClipboardContent {
    Image(DynamicImage),
    Text(String),
}
pub fn clipboard() -> Result<ClipboardContent, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Cannot access clipboard: {e}"))?;
    if let Ok(data) = clipboard.get_image() {
        if data.width as u64 * data.height as u64 > MAX_PIXELS {
            return Err("Clipboard image exceeds 40 megapixels.".into());
        }
        let image = RgbaImage::from_raw(
            data.width as u32,
            data.height as u32,
            data.bytes.into_owned(),
        )
        .ok_or("Invalid clipboard image pixels")?;
        return Ok(ClipboardContent::Image(DynamicImage::ImageRgba8(image)));
    }
    clipboard
        .get_text()
        .map(ClipboardContent::Text)
        .map_err(|_| "The clipboard contains neither a supported image nor text.".into())
}
pub fn pasted_image_path(text: &str) -> Option<PathBuf> {
    let path = PathBuf::from(text.trim().trim_matches('\'').trim_matches('"'));
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    (matches!(ext.as_str(), "png" | "jpg" | "jpeg") && path.is_file()).then_some(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_kitty_transmits_png_and_cleans_up_its_own_placement() {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        let dir = std::env::temp_dir().join(format!("sprout-direct-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("image.png");
        RgbaImage::from_pixel(40, 40, image::Rgba([25, 80, 200, 255]))
            .save(&path)
            .unwrap();
        let mut preview = Preview::load(&path, "Test".into(), &picker).unwrap();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(90, 30)).unwrap();
        terminal
            .draw(|f| preview.draw(f, Rect::new(10, 4, 50, 20)))
            .unwrap();
        let cells = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(!cells.contains('\u{10eeee}'));
        let mut renderer = KittyRenderer::default();
        let mut output = Vec::new();
        renderer.render_to(Some(&preview), &mut output).unwrap();
        let wire = String::from_utf8(output.clone()).unwrap();
        assert!(wire.contains("a=T,f=100,t=d"));
        assert!(!wire.contains("U=1"));
        let encoded = wire
            .split("\x1b_G")
            .skip(1)
            .map(|chunk| {
                chunk
                    .split_once(';')
                    .unwrap()
                    .1
                    .split("\x1b\\")
                    .next()
                    .unwrap()
            })
            .collect::<String>();
        let png = base64_simd::STANDARD
            .decode_to_vec(encoded.as_bytes())
            .unwrap();
        assert_eq!(
            image::load_from_memory(&png)
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0),
            &image::Rgba([25, 80, 200, 255])
        );
        output.clear();
        renderer.render_to(Some(&preview), &mut output).unwrap();
        assert!(output.is_empty());
        preview.frame_area = Some(Rect::new(20, 8, 4, 2));
        renderer.render_to(Some(&preview), &mut output).unwrap();
        assert!(String::from_utf8_lossy(&output).contains("a=d,d=I"));
        output.clear();
        renderer.render_to(None, &mut output).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!("\x1b_Ga=d,d=I,i={},q=2\x1b\\", renderer.id)
        );
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn multiple_thumbnails_have_separate_placements_and_cleanup() {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        let dir = std::env::temp_dir().join(format!("sprout-thumbs-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("image.png");
        RgbaImage::from_pixel(20, 20, image::Rgba([25, 80, 200, 255]))
            .save(&path)
            .unwrap();
        let mut one = Preview::load(&path, "One".into(), &picker).unwrap();
        let mut two = Preview::load(&path, "Two".into(), &picker).unwrap();
        one.frame_area = Some(Rect::new(1, 1, 4, 2));
        two.frame_area = Some(Rect::new(8, 1, 4, 2));
        let mut renderer = KittyRenderer::default();
        let mut bytes = Vec::new();
        renderer.render_many_to(&[&one, &two], &mut bytes).unwrap();
        assert_eq!(renderer.shown.len(), 2);
        assert_eq!(String::from_utf8_lossy(&bytes).matches("a=T").count(), 2);
        bytes.clear();
        renderer.render_many_to(&[&two], &mut bytes).unwrap();
        assert_eq!(renderer.shown.len(), 1);
        let wire = String::from_utf8_lossy(&bytes);
        assert_eq!(wire.matches("a=d").count(), 1);
        assert!(!wire.contains("a=T"));
        renderer.render_many_to(&[], &mut Vec::new()).unwrap();
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn image_fits_rect_without_dividing_by_zero() {
        assert_eq!(fit_area(Rect::new(0, 0, 0, 3), 100, 100, (10, 20)), None);
        let area = Rect::new(10, 5, 20, 10);
        let fitted = fit_area(area, 718, 956, (10, 20)).unwrap();
        assert!(fitted.x >= area.x && fitted.right() <= area.right());
        assert!(fitted.y >= area.y && fitted.bottom() <= area.bottom());
    }
    #[test]
    fn staged_png_keeps_pixels_and_preview_loads() {
        let dir = std::env::temp_dir().join(format!("sprout-images-test-{}", std::process::id()));
        let cfg = Config {
            team: "test".into(),
            board: String::new(),
            demo: false,
            node: "node".into(),
            adapter: dir.join("board-runtime/adapter.mjs"),
            data_dir: None,
        };
        let original = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            48,
            32,
            image::Rgba([12, 34, 56, 255]),
        ));
        let staged = stage(&cfg, "sprout-test", original.clone(), "Test.png".into()).unwrap();
        assert_eq!(
            decode(&staged.path).unwrap().to_rgba8(),
            original.to_rgba8()
        );
        assert!(Preview::load(&staged.path, "Test".into(), &Picker::halfblocks()).is_ok());
        cleanup(&cfg, "sprout-test");
        assert!(!staged.path.exists());
        let _ = fs::remove_dir_all(dir);
    }
}
