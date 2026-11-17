//! Local profile image preferences and their preview card.

use egui::{Color32, Id, Rect, RichText, Sense, TextureHandle, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    photo: Vec<u8>,
    banner: Vec<u8>,
    image: Vec<u8>,
    #[serde(skip)]
    pub(crate) error: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) enum Slot {
    Photo,
    Banner,
    Image,
}

impl Profile {
    fn image(&mut self, slot: Slot) -> &mut Vec<u8> {
        match slot {
            Slot::Photo => &mut self.photo,
            Slot::Banner => &mut self.banner,
            Slot::Image => &mut self.image,
        }
    }

    pub(crate) fn import(&mut self, slot: Slot, path: &str, portuguese: bool) -> Result<(), String> {
        use std::io::Read as _;
        const MAX_BYTES: u64 = 20 * 1024 * 1024;
        let error = || {
            if portuguese { "Escolha uma imagem PNG ou JPEG válida de até 20 MB." } else { "Choose a valid PNG or JPEG image up to 20 MB." }
                .to_owned()
        };
        let path = std::path::Path::new(path);
        if !path.is_file() {
            return Err(error());
        }
        let mut data = Vec::new();
        std::fs::File::open(path).and_then(|file| file.take(MAX_BYTES + 1).read_to_end(&mut data)).map_err(|_| error())?;
        if data.len() as u64 > MAX_BYTES {
            return Err(error());
        }
        let format = image::guess_format(&data).map_err(|_| error())?;
        if !matches!(format, image::ImageFormat::Jpeg | image::ImageFormat::Png) {
            return Err(error());
        }
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(data), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader.decode().map_err(|_| error())?;
        let preview = match slot {
            Slot::Photo => decoded.resize_to_fill(144, 144, image::imageops::FilterType::Triangle),
            Slot::Banner => decoded.resize_to_fill(960, 250, image::imageops::FilterType::Triangle),
            Slot::Image => decoded.thumbnail(640, 320),
        };
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&preview.to_rgb8()).map_err(|_| error())?;
        *self.image(slot) = jpeg;
        Ok(())
    }
}

fn preview(ui: &Ui, id: Id, rect: Rect, bytes: &[u8]) -> bool {
    use std::hash::{DefaultHasher, Hash as _, Hasher as _};
    if bytes.is_empty() || bytes.len() > 500_000 {
        ui.ctx().data_mut(|data| data.remove::<(u64, TextureHandle)>(id));
        return false;
    }
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let hash = hasher.finish();
    let cached = ui.ctx().data_mut(|data| data.get_temp::<(u64, TextureHandle)>(id));
    let texture = if let Some((_, texture)) = cached.filter(|(previous, _)| *previous == hash) {
        texture
    } else {
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Jpeg);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(960);
        limits.max_image_height = Some(320);
        limits.max_alloc = Some(8 * 1024 * 1024);
        reader.limits(limits);
        let Ok(image) = reader.decode() else { return false };
        let rgba = image.into_rgba8();
        let pixels = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba);
        let texture = ui.ctx().load_texture("profile", pixels, egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|data| data.insert_temp(id, (hash, texture.clone())));
        texture
    };
    let scale = (rect.width() / texture.size_vec2().x).min(rect.height() / texture.size_vec2().y);
    let fitted = Rect::from_center_size(rect.center(), texture.size_vec2() * scale);
    ui.painter().image(texture.id(), fitted, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    true
}

pub(crate) fn card(ui: &mut Ui, profile: &mut Profile, name: &mut String, portuguese: bool, can_import: bool, can_edit_name: bool) -> Option<Slot> {
    let label = |pt, en| if portuguese { pt } else { en };
    let colors = crate::theme::Tokens::get(ui.ctx());
    let mut pick = None;
    ui.push_id("local_profile", |ui| {
        let width = ui.available_width().clamp(120.0, 570.0);
        ui.heading(label("Meu perfil", "My profile"));
        ui.add_space(6.0);
        let (banner, _) = ui.allocate_exact_size(vec2(width, width * 0.26), Sense::hover());
        ui.painter().rect_filled(banner, 8.0, colors.accent);
        preview(ui, ui.scope_id().with("banner"), banner, &profile.banner);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let (photo, _) = ui.allocate_exact_size(vec2(62.0, 62.0), Sense::hover());
            if !preview(ui, ui.scope_id().with("photo"), photo, &profile.photo) {
                ui.painter().rect_filled(photo, 8.0, colors.accent);
                let initials: String = name.split_whitespace().take(2).filter_map(|part| part.chars().next()).flat_map(char::to_uppercase).collect();
                ui.painter().text(photo.center(), egui::Align2::CENTER_CENTER, initials, egui::FontId::proportional(22.0), Color32::WHITE);
            }
            ui.vertical(|ui| {
                let name_label = ui.label(RichText::new(label("Nome", "Name")).strong());
                ui.add_enabled(can_edit_name, egui::TextEdit::singleline(name).char_limit(100).desired_width(240.0)).labelled_by(name_label.id);
            });
        });
        ui.add_space(6.0);
        for (slot, title, action) in [
            (Slot::Photo, label("Foto", "Photo"), label("Escolher foto", "Choose photo")),
            (Slot::Banner, label("Banner", "Banner"), label("Escolher banner", "Choose banner")),
            (Slot::Image, label("Imagem adicional", "Additional image"), label("Escolher imagem", "Choose image")),
        ] {
            let data = profile.image(slot);
            ui.push_id(title, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(can_import, egui::Button::new(action)).clicked() {
                        pick = Some(slot);
                    }
                    if !data.is_empty() && ui.button(label("Remover", "Remove")).clicked() {
                        data.clear();
                    }
                });
            });
        }
        if pick.is_some() {
            profile.error = None;
        }
        if let Some(error) = &profile.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if !profile.image.is_empty() {
            ui.add_space(6.0);
            let (rect, _) = ui.allocate_exact_size(vec2(width.min(330.0), 154.0), Sense::hover());
            preview(ui, ui.scope_id().with("image"), rect, &profile.image);
        }
    });
    pick
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_keeps_independent_slots_and_serializes_saved_image() {
        let folder = std::env::temp_dir().join(format!("craft-profile-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("profile.png");
        image::RgbImage::from_pixel(32, 24, image::Rgb([40, 80, 180])).save(&path).unwrap();
        let mut profile = Profile::default();
        profile.import(Slot::Photo, path.to_str().unwrap(), true).unwrap();
        let serialized = serde_json::to_string(&profile).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        let mut restored: Profile = serde_json::from_str(&serialized).unwrap();
        assert!(restored.banner.is_empty() && restored.image.is_empty());
        let thumb = image::load_from_memory(&restored.photo).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (144, 144));
        assert!(restored.import(Slot::Photo, path.to_str().unwrap(), true).is_err());
        assert_eq!(restored.photo, profile.photo);
    }
}
