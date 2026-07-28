//! 封面缩略图：提取内嵌封面，缩放并合成为正方形。
//!
//! ## 为什么要合成正方形
//!
//! 界面上的封面卡是正方形，但实测约四成封面不是（1300×910、710×1000 这类，
//! 同人专辑常用宽幅插画）。裁剪会切掉画面主体，留白会破坏网格的视觉节奏，
//! 因此采用「原图完整居中 + 同图放大模糊填充四周」——Apple Music 处理非方图的做法。
//!
//! 合成放在**后端一次性完成**，而不是前端用两层 DOM 加 CSS 模糊：专辑网格里会同时
//! 出现几十张封面，每张都挂一个模糊图层意味着每帧重算，滚动必然掉帧。合成成正方形
//! 之后前端就是一张普通图片。
//!
//! ## 缩放
//!
//! - **只缩小，不放大**。原图比档位还小就按原尺寸存：放大不产生任何信息，只让文件
//!   更大、解码更慢；真需要放大时交给 GPU 在合成阶段做（CSS 默认就是平滑插值）。
//! - **缩小用 Lanczos3，且逐档接力**（原图 → 1024 → 512 → 128）。Lanczos 的滤波核
//!   开销随缩放比线性增长，一张 4000×4000 直接缩到 128 是 31 倍，核半径会撑到 90
//!   多像素；分档接力后每步都在 2~4 倍内，总成本远低于逐档从原图各缩一次，而 2 倍
//!   步长下的累积误差可以忽略。
//! - 不用 Nearest / Triangle：封面常有细密纹理和小字，欠采样会走样闪烁；
//!   不用 Gaussian：发糊。Lanczos3 在高对比边缘有轻微振铃，对插画和照片可以接受。
//!
//! 唯一封面数远少于曲目数（按内容指纹去重，实测 939 首只对应 33 张封面），
//! 所以整库扫描期间的解码开销可以忽略。

use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};

/// 缩略图档位（正方形边长，物理像素）。
///
/// 依据界面上封面的实际显示尺寸按 2 倍屏推算：列表与播放条 26~48px → 128；
/// 专辑网格卡 200~260px → 512；专辑详情 232px、歌词页 300px → 1024。
pub const SIZES: &[u32] = &[128, 512, 1024];

/// 宽高比偏离 1 不超过这个比例就当成正方形，不做模糊合成。
/// 1000×987、1424×1411 这类「几乎是方的」封面没必要多绕一圈。
const SQUARE_TOLERANCE: f32 = 0.02;

/// 模糊强度相对画布边长的比例。固定像素值不行：128 档上用大半径会糊成一块纯色。
const BLUR_SIGMA_RATIO: f32 = 0.02;

/// 背景模糊在这个尺寸上计算后再放大。模糊本就丢高频，在小图上算视觉等价而成本极低。
const BLUR_WORK_SIZE: u32 = 96;

/// 中位色采样尺寸（取自 LGP3 的同名处理，用于给画布打底）。
const MEDIAN_SAMPLE: u32 = 64;

/// 某个封面指纹在缓存目录中的文件路径。
pub fn thumb_path(dir: &Path, cover_key: &str, size: u32) -> PathBuf {
    dir.join(format!("{cover_key}-{size}.jpg"))
}

/// 该封面的所有档位是否都已生成（重扫时据此跳过解码）。
pub fn thumbs_exist(dir: &Path, cover_key: &str) -> bool {
    SIZES
        .iter()
        .all(|&s| thumb_path(dir, cover_key, s).exists())
}

/// 解码封面字节并写出各档缩略图。
///
/// 失败只返回 Err 由调用方记录：封面缺失不该让整次扫描失败，界面回落占位渐变即可。
pub fn write_thumbs(dir: &Path, cover_key: &str, bytes: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建封面目录失败: {e}"))?;
    let decoded = image::load_from_memory(bytes).map_err(|e| format!("封面解码失败: {e}"))?;
    // 统一转成不透明 RGB：JPEG 不带 alpha，透明区域若不先合成会变成黑块。
    let mut current = flatten(decoded);

    // 从大档到小档接力缩小，每一步都以上一档为输入。
    for &size in SIZES.iter().rev() {
        current = shrink_to_fit(current, size);
        let square = squarify(&current, size);
        let path = thumb_path(dir, cover_key, size);
        square
            .save_with_format(&path, image::ImageFormat::Jpeg)
            .map_err(|e| format!("封面写入失败 {}: {e}", path.display()))?;
    }
    Ok(())
}

/// 透明通道合成到白底。封面几乎不用透明，但 PNG 封面偶有 alpha，
/// 直接丢弃会让透明处变黑。
fn flatten(img: DynamicImage) -> RgbImage {
    if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let mut out = RgbImage::new(rgba.width(), rgba.height());
        for (x, y, p) in rgba.enumerate_pixels() {
            let a = p[3] as f32 / 255.0;
            let mix = |c: u8| (c as f32 * a + 255.0 * (1.0 - a)).round() as u8;
            out.put_pixel(x, y, Rgb([mix(p[0]), mix(p[1]), mix(p[2])]));
        }
        out
    } else {
        img.to_rgb8()
    }
}

/// 按长边缩到 `size` 以内，保持宽高比。**比 `size` 小的原样返回，不放大。**
fn shrink_to_fit(img: RgbImage, size: u32) -> RgbImage {
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= size {
        return img;
    }
    DynamicImage::ImageRgb8(img)
        .resize(size, size, FilterType::Lanczos3)
        .to_rgb8()
}

/// 合成正方形：原图居中完整显示，四周用同图放大模糊填充。
///
/// 画布边长取「长边」而不是档位值——原图比档位小时不放大（见模块文档），
/// 此时画布也应随之缩小，否则等于把小图放大。
fn squarify(img: &RgbImage, _size: u32) -> RgbImage {
    let (w, h) = (img.width(), img.height());
    let side = w.max(h);
    let ratio = w as f32 / h as f32;
    if (ratio - 1.0).abs() <= SQUARE_TOLERANCE {
        // 已经（几乎）是正方形：宽高相差不到 2%，拉成正方形肉眼无感，
        // 省掉一次模糊合成。
        return DynamicImage::ImageRgb8(img.clone())
            .resize_exact(side, side, FilterType::Lanczos3)
            .to_rgb8();
    }

    // 底色：整图中位色。模糊背景在圆角与边缘处可能偏亮或偏暗，先铺一层同色调的底
    // 能避免露出突兀的边（LGP3 的 CoverImageProcessor 同样先填中位色）。
    let median = median_color(img);
    let mut canvas = RgbImage::from_pixel(side, side, median);

    // 背景层：aspect-fill 裁成正方形 → 模糊 → 放大到画布。
    // 先裁成填满画布的正方形再模糊，模糊核就不会吸到画布外的空白（Core Image 侧
    // 需要 CIAffineClamp 才能避免的暗边，在这里因为图已填满而不存在）。
    let small = DynamicImage::ImageRgb8(img.clone())
        .resize_to_fill(BLUR_WORK_SIZE, BLUR_WORK_SIZE, FilterType::Triangle)
        .to_rgb8();
    let blurred = image::imageops::blur(&small, BLUR_WORK_SIZE as f32 * BLUR_SIGMA_RATIO * 4.0);
    let background = DynamicImage::ImageRgb8(blurred)
        .resize_exact(side, side, FilterType::Triangle)
        .to_rgb8();
    image::imageops::overlay(&mut canvas, &background, 0, 0);

    // 前景层：原图居中，一个像素都不裁。
    let (dx, dy) = (((side - w) / 2) as i64, ((side - h) / 2) as i64);
    image::imageops::overlay(&mut canvas, img, dx, dy);
    canvas
}

/// 整图中位色：缩到 64×64 后各通道取中位数。
/// 用中位数而不是均值，是因为均值会被大面积高光或暗部拉偏。
fn median_color(img: &RgbImage) -> Rgb<u8> {
    let small = DynamicImage::ImageRgb8(img.clone())
        .resize_exact(MEDIAN_SAMPLE, MEDIAN_SAMPLE, FilterType::Triangle)
        .to_rgb8();
    let mut ch: [Vec<u8>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for p in small.pixels() {
        for i in 0..3 {
            ch[i].push(p[i]);
        }
    }
    let mid = |v: &mut Vec<u8>| {
        v.sort_unstable();
        v[v.len() / 2]
    };
    Rgb([mid(&mut ch[0]), mid(&mut ch[1]), mid(&mut ch[2])])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, _| Rgb([(x % 256) as u8, 120, 200]))
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn shrink_never_upscales() {
        let small = img(300, 200);
        let out = shrink_to_fit(small, 1024);
        assert_eq!(
            (out.width(), out.height()),
            (300, 200),
            "比档位小的原图不该被放大"
        );
    }

    #[test]
    fn shrink_keeps_aspect_ratio() {
        let out = shrink_to_fit(img(2000, 1000), 512);
        assert_eq!(out.width(), 512);
        assert_eq!(out.height(), 256);
    }

    /// 非正方形要补成正方形，且原图一个像素都不能裁。
    #[test]
    fn squarify_pads_without_cropping() {
        let src = img(400, 200);
        let out = squarify(&src, 400);
        assert_eq!((out.width(), out.height()), (400, 400));
        // 原图居中：中心行的像素应与源图对应像素一致（未被裁剪或缩放）。
        let dy = (400 - 200) / 2;
        assert_eq!(out.get_pixel(10, dy + 10), src.get_pixel(10, 10));
    }

    /// 几乎是正方形（差 2% 以内）的走快路径，不做模糊合成。
    #[test]
    fn near_square_skips_compositing() {
        let out = squarify(&img(1000, 987), 1000);
        assert_eq!((out.width(), out.height()), (1000, 1000));
    }

    #[test]
    fn writes_all_sizes_and_reports_existence() {
        let d = tmpdir("shannon_cover_sizes");
        let mut bytes: Vec<u8> = Vec::new();
        DynamicImage::ImageRgb8(img(1300, 910))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();

        assert!(!thumbs_exist(&d, "k1"));
        write_thumbs(&d, "k1", &bytes).unwrap();
        assert!(thumbs_exist(&d, "k1"), "各档缩略图都应生成");
        for &s in SIZES {
            let out = image::open(thumb_path(&d, "k1", s)).unwrap();
            assert_eq!(out.width(), out.height(), "每一档都必须是正方形");
            assert!(out.width() <= s);
        }
        let _ = std::fs::remove_dir_all(d);
    }

    /// 坏数据只报错，不 panic——封面读不出不该让整次扫描失败。
    #[test]
    fn garbage_bytes_report_error() {
        let d = tmpdir("shannon_cover_garbage");
        assert!(write_thumbs(&d, "k", b"not an image").is_err());
        let _ = std::fs::remove_dir_all(d);
    }
}
