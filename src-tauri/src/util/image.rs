//! Кодирование изображений в PNG / data URL без тяжёлых зависимостей.

use base64::Engine;

pub fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
        return None;
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgba[..(width * height * 4) as usize]).ok()?;
    }
    Some(out)
}

pub fn png_data_url(width: u32, height: u32, rgba: &[u8]) -> Option<String> {
    encode_png_rgba(width, height, rgba).map(|png| bytes_data_url("image/png", &png))
}

pub fn bytes_data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// BGRA с премультиплицированной альфой (GDI) → обычный RGBA.
/// Если альфа-канал полностью нулевой (bitmap без альфы) — делаем непрозрачным.
pub fn unpremultiply_bgra_to_rgba(buf: &mut [u8]) {
    let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
    for px in buf.chunks_exact_mut(4) {
        let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
        if !has_alpha {
            px[0] = r;
            px[2] = b;
            px[3] = 255;
            continue;
        }
        if a == 0 {
            px.copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
        px[0] = un(r);
        px[1] = un(g);
        px[2] = un(b);
    }
}

/// Уменьшает RGBA-изображение (ближайший сосед) так, чтобы большая сторона ≤ max.
pub fn downscale_rgba(width: u32, height: u32, rgba: &[u8], max: u32) -> (u32, u32, Vec<u8>) {
    if width <= max && height <= max {
        return (width, height, rgba.to_vec());
    }
    let scale = max as f64 / width.max(height) as f64;
    let nw = ((width as f64 * scale).round() as u32).max(1);
    let nh = ((height as f64 * scale).round() as u32).max(1);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        let sy = ((y as f64 / scale) as u32).min(height - 1);
        for x in 0..nw {
            let sx = ((x as f64 / scale) as u32).min(width - 1);
            let si = ((sy * width + sx) * 4) as usize;
            let di = ((y * nw + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
        }
    }
    (nw, nh, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_roundtrip_header() {
        let px = vec![255u8; 4 * 4 * 4];
        let png = encode_png_rgba(4, 4, &px).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert!(encode_png_rgba(4, 4, &px[..10]).is_none());
    }

    #[test]
    fn downscale_keeps_aspect() {
        let px = vec![0u8; 400 * 200 * 4];
        let (w, h, data) = downscale_rgba(400, 200, &px, 100);
        assert_eq!((w, h), (100, 50));
        assert_eq!(data.len(), 100 * 50 * 4);
    }

    #[test]
    fn unpremultiply_without_alpha_is_opaque() {
        let mut px = vec![10, 20, 30, 0];
        unpremultiply_bgra_to_rgba(&mut px);
        assert_eq!(px, vec![30, 20, 10, 255]);
    }
}
