use anyhow::{bail, Context, Result};
use metaflac::block::PictureType;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct MetadataInput<'a> {
    pub file_path: &'a Path,
    pub cover_path: &'a Path,
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
    pub album_artist: &'a str,
    pub track_number: Option<i64>,
    pub comment: Option<&'a str>,
    pub lyrics_path: &'a Path,
}

/// 按扩展名分派元数据写入，返回最终文件路径（wav 转 flac 后路径会变）。
pub fn apply_metadata(input: &MetadataInput) -> Result<PathBuf> {
    let ext = input
        .file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "wav" => {
            let flac_path = transcode_wav_to_flac(input.file_path)?;
            write_flac_tags(&MetadataInput {
                file_path: &flac_path,
                ..*input
            })?;
            Ok(flac_path)
        }
        "flac" => {
            write_flac_tags(input)?;
            Ok(input.file_path.to_path_buf())
        }
        "mp3" => {
            ensure_mp3_tags(input)?;
            Ok(input.file_path.to_path_buf())
        }
        _ => Ok(input.file_path.to_path_buf()),
    }
}

/// WAV → FLAC 转码：调系统 ffmpeg（需本机安装）。标签与封面由随后的 metaflac 写入，ffmpeg 只负责音频。
fn transcode_wav_to_flac(wav_path: &Path) -> Result<PathBuf> {
    let flac_path = wav_path.with_extension("flac");
    let status = Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(wav_path)
        .arg(&flac_path)
        .output()
        .context("启动 ffmpeg 失败（请确认已安装 ffmpeg）")?;
    if !status.status.success() {
        bail!(
            "ffmpeg 转码失败（{}）：{}",
            status.status,
            String::from_utf8_lossy(&status.stderr).chars().rev().take(800).collect::<String>().chars().rev().collect::<String>()
        );
    }
    std::fs::remove_file(wav_path).with_context(|| format!("删除原 WAV 失败：{}", wav_path.display()))?;
    Ok(flac_path)
}

fn read_lyrics_text(lyrics_path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(lyrics_path).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn write_flac_tags(input: &MetadataInput) -> Result<()> {
    let mut tag = metaflac::Tag::read_from_path(input.file_path)
        .with_context(|| format!("读取 FLAC 失败：{}", input.file_path.display()))?;
    let vc = tag.vorbis_comments_mut();
    vc.set("TITLE", vec![input.title]);
    vc.set("ARTIST", vec![input.artist]);
    vc.set("ALBUM", vec![input.album]);
    vc.set("ALBUMARTIST", vec![input.album_artist]);
    if let Some(track) = input.track_number {
        vc.set("TRACKNUMBER", vec![track.to_string()]);
    }
    if let Some(comment) = input.comment {
        vc.set("DESCRIPTION", vec![comment]);
    }
    if let Some(lyrics) = read_lyrics_text(input.lyrics_path) {
        vc.set("LYRICS", vec![lyrics]);
    }
    if let Some(cover) = read_cover(input.cover_path) {
        // add_picture 内部会先移除同类型封面块，不会重复嵌入
        let mime = cover_mime(&cover);
        tag.add_picture(mime, PictureType::CoverFront, cover);
    }
    tag.write_to_path(input.file_path)
        .with_context(|| format!("写入 FLAC 标签失败：{}", input.file_path.display()))?;
    Ok(())
}

fn read_cover(cover_path: &Path) -> Option<Vec<u8>> {
    std::fs::read(cover_path).ok()
}

// coverUrl 下发的 cover.jpg 不一定是 JPEG 字节，按魔数判断 MIME
fn cover_mime(data: &[u8]) -> &'static str {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else {
        "image/jpeg"
    }
}

fn ensure_mp3_tags(input: &MetadataInput) -> Result<()> {
    // 裸 MP3（无任何 ID3 标签）读取会报 NoTag，no_tag_ok 将其映射为 None，等价于空标签
    let existing = match id3::no_tag_ok(id3::Tag::read_from_path(input.file_path)) {
        Ok(Some(tag)) => tag,
        Ok(None) => id3::Tag::new(),
        Err(e) => return Err(e).with_context(|| format!("读取 MP3 失败：{}", input.file_path.display())),
    };
    // 已有 title/artist/album/封面则跳过（与旧版 music-metadata 检查逻辑一致）
    let has_all = existing.title().is_some()
        && existing.artist().is_some()
        && existing.album().is_some()
        && existing.pictures().next().is_some();
    if has_all {
        return Ok(());
    }

    let mut tag = existing;
    use id3::TagLike;
    tag.set_title(input.title);
    tag.set_artist(input.artist);
    tag.set_album(input.album);
    tag.set_text("TPE2", input.album_artist);
    if let Some(track) = input.track_number {
        tag.set_text("TRCK", track.to_string());
    }
    if let Some(comment) = input.comment {
        tag.add_frame(id3::frame::Comment {
            lang: "eng".to_string(),
            description: String::new(),
            text: comment.to_string(),
        });
    }
    if let Some(lyrics) = read_lyrics_text(input.lyrics_path) {
        tag.add_frame(id3::frame::Lyrics {
            lang: "eng".to_string(),
            description: String::new(),
            text: lyrics,
        });
    }
    if let Some(cover) = read_cover(input.cover_path) {
        tag.add_frame(id3::frame::Picture {
            mime_type: cover_mime(&cover).to_string(),
            picture_type: id3::frame::PictureType::CoverFront,
            description: "Cover".to_string(),
            data: cover,
        });
    }
    tag.write_to_path(input.file_path, id3::Version::Id3v24)
        .with_context(|| format!("写入 MP3 标签失败：{}", input.file_path.display()))?;
    Ok(())
}
