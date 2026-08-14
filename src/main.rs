mod api;
mod db;
mod download;
mod metadata;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const DEFAULT_GAME: &str = "arknights";

fn sanitize_filename(name: &str, replacement: &str) -> String {
    // ponytail: 手写清洗已覆盖 filenamify 的核心行为（Windows 非法字符 + 控制字符），专辑目录名沿用旧版不清洗
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let illegal = matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            || c.is_control();
        out.push(if illegal { '\u{0}' } else { c });
    }
    // 占位后再统一替换，避免 replacement 本身含非法字符时引入新问题
    out.replace('\u{0}', replacement)
}

fn ext_from_url(url: &str) -> String {
    url.rsplit('/').next()
        .and_then(|last| last.rsplit_once('.'))
        .map(|(_, ext)| ext.to_lowercase())
        .filter(|ext| !ext.is_empty() && ext.len() <= 5 && !ext.contains('?'))
        .unwrap_or_default()
}

fn album_dir(save_path: &str, album: &db::Album) -> PathBuf {
    Path::new(save_path).join(album.belong.as_deref().unwrap_or(DEFAULT_GAME)).join(&album.name)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let save_path = std::env::var("SAVE_PATH").context("请设置保存路径（SAVE_PATH）")?;
    let replacement = std::env::var("REPLACE_CHAR").unwrap_or_else(|_| "_".to_string());

    let client = reqwest::Client::new();
    // ponytail: SQLite 调用是同步阻塞的，CLI 顺序流程无所谓；并发下载时再上 spawn_blocking
    let conn = db::open()?;

    let albums = api::get_albums(&client).await?;
    let songs = api::get_songs(&client).await?;
    println!("官网共有{}个专辑，{}首歌曲", albums.len(), songs.list.len());

    ingest_albums(&conn, &albums)?;
    insert_songs(&conn, &songs.list)?;
    fetch_album_details(&client, &conn).await?;
    download_covers(&client, &conn, &save_path).await?;
    fetch_song_details(&client, &conn).await?;
    download_and_tag_songs(&client, &conn, &save_path, &replacement).await?;
    fill_metadata_for_existing(&conn, &save_path, &replacement)?;
    println!("全部完成");
    Ok(())
}

fn ingest_albums(conn: &rusqlite::Connection, albums: &[api::AlbumListItem]) -> Result<()> {
    for album in albums {
        if db::album_exists(conn, album.cid)? {
            continue;
        }
        db::insert_album(conn, album.cid, &album.name, &album.cover_url, &album.artistes.join(","))?;
    }
    Ok(())
}

fn insert_songs(conn: &rusqlite::Connection, songs: &[api::SongListItem]) -> Result<()> {
    for song in songs {
        if db::song_exists(conn, song.cid)? {
            continue;
        }
        db::insert_song(conn, song.cid, &song.name, song.album_cid, &song.artists.join(","))?;
    }
    Ok(())
}

async fn fetch_album_details(client: &reqwest::Client, conn: &rusqlite::Connection) -> Result<()> {
    let albums = db::albums_missing_intro(conn)?;
    if albums.is_empty() {
        return Ok(());
    }
    println!("共有{}个专辑待拉取详情", albums.len());
    for (i, album) in albums.iter().enumerate() {
        println!("正在拉取专辑详情：{}，{}/{}", album.name, i + 1, albums.len());
        match api::get_album_detail(client, album.cid).await {
            Ok(detail) => {
                // intro 空串作为"已拉取但无简介"的哨兵，避免反复重试
                db::update_album_detail(
                    conn,
                    album.cid,
                    detail.intro.as_deref().unwrap_or(""),
                    detail.belong.as_deref().unwrap_or(DEFAULT_GAME),
                    detail.cover_de_url.as_deref(),
                )?;
                if let Some(songs) = &detail.songs {
                    for (idx, s) in songs.iter().enumerate() {
                        if db::song_exists(conn, s.cid)? {
                            db::set_track_number(conn, s.cid, (idx + 1) as i64)?;
                        }
                    }
                }
            }
            Err(e) => eprintln!("拉取专辑详情失败：{}：{e:#}", album.name),
        }
    }
    println!("专辑详情拉取完成");
    Ok(())
}

async fn download_covers(client: &reqwest::Client, conn: &rusqlite::Connection, save_path: &str) -> Result<()> {
    let albums = db::all_albums(conn)?;
    let need_cover: Vec<_> = albums.iter().filter(|a| !a.is_cover_downloaded).collect();
    let need_cover_de: Vec<_> = albums.iter().filter(|a| a.cover_de_url.is_some() && !a.is_cover_de_downloaded).collect();
    println!("共有{}个专辑封面未下载，{}个高清封面未下载", need_cover.len(), need_cover_de.len());
    let total = need_cover.len() + need_cover_de.len();
    let mut count = 0;
    for album in &need_cover {
        match download::download_file(client, &album.cover_url, &album_dir(save_path, album).join("cover.jpg")).await {
            Ok(()) => {
                db::set_cover_downloaded(conn, album.cid, false)?;
                count += 1;
                println!("封面下载完成：{}，{}/{}", album.name, count, total);
            }
            Err(e) => eprintln!("封面下载失败：{}：{e:#}", album.name),
        }
    }
    for album in &need_cover_de {
        let url = album.cover_de_url.as_deref().unwrap();
        let ext = ext_from_url(url);
        let ext = if ext.is_empty() { "jpg" } else { &ext };
        let dest = album_dir(save_path, album).join(format!("cover_de.{ext}"));
        match download::download_file(client, url, &dest).await {
            Ok(()) => {
                db::set_cover_downloaded(conn, album.cid, true)?;
                count += 1;
                println!("高清封面下载完成：{}，{}/{}", album.name, count, total);
            }
            Err(e) => eprintln!("高清封面下载失败：{}：{e:#}", album.name),
        }
    }
    println!("封面下载完成");
    Ok(())
}

async fn fetch_song_details(client: &reqwest::Client, conn: &rusqlite::Connection) -> Result<()> {
    let songs = db::songs_missing_source(conn)?;
    println!("共有{}首歌曲未获取下载地址", songs.len());
    for (i, song) in songs.iter().enumerate() {
        println!("正在获取歌曲详情：{}，当前进度：{}/{}", song.name, i + 1, songs.len());
        match api::get_song_detail(client, song.cid).await {
            Ok(detail) => {
                db::update_song_source(conn, song.cid, &detail.source_url, detail.lyric_url.as_deref())?;
            }
            Err(e) => eprintln!("获取歌曲详情失败：{}：{e:#}", song.name),
        }
    }
    Ok(())
}

async fn download_and_tag_songs(
    client: &reqwest::Client,
    conn: &rusqlite::Connection,
    save_path: &str,
    replacement: &str,
) -> Result<()> {
    let songs = db::songs_not_downloaded(conn)?;
    println!("共有{}首歌曲未下载", songs.len());
    for (i, song) in songs.iter().enumerate() {
        println!("正在下载歌曲：{}，当前进度：{}/{}", song.name, i + 1, songs.len());
        let album = match db::album_by_cid(conn, song.album_cid)? {
            Some(a) => a,
            None => {
                eprintln!("找不到所属专辑，跳过：{}", song.name);
                continue;
            }
        };
        let Some(initial_url) = &song.source_url else {
            eprintln!("无下载地址，跳过：{}", song.name);
            continue;
        };
        let dir = album_dir(save_path, &album);
        let safe_name = sanitize_filename(&song.name, replacement);
        let ext = ext_from_url(initial_url);
        let song_path = dir.join(format!("{safe_name}.{ext}"));
        let lyrics_path = dir.join(format!("{safe_name}.lrc"));

        // CDN 签名 URL 带过期时间（路径里的 /69FDD850/ 段），库里的旧 URL 过期后 403：
        // 下载失败时重新拉一次 song detail 刷新 URL 并重试
        let mut lyric_url = song.lyric_url.clone();
        if let Err(e) = download::download_file(client, initial_url, &song_path).await {
            eprintln!("歌曲下载失败，刷新下载地址后重试：{}：{e:#}", song.name);
            let detail = match api::get_song_detail(client, song.cid).await {
                Ok(d) => d,
                Err(e2) => {
                    eprintln!("重新获取歌曲详情失败：{}：{e2:#}", song.name);
                    continue;
                }
            };
            db::update_song_source(conn, song.cid, &detail.source_url, detail.lyric_url.as_deref())?;
            lyric_url = detail.lyric_url.clone();
            if let Err(e) = download::download_file(client, &detail.source_url, &song_path).await {
                eprintln!("下载歌曲失败：{}：{e:#}", song.name);
                continue;
            }
        }
        if let Some(lyric_url) = &lyric_url {
            if let Err(e) = download::download_file(client, lyric_url, &lyrics_path).await {
                eprintln!("下载歌词失败：{}：{e:#}", song.name);
            }
        }
        let song_path_str = song_path.to_string_lossy().to_string();
        db::set_song_downloaded(conn, song.cid, &song_path_str)?;

        // 下载成功后立即写元数据；失败留给回填阶段重试
        let result = metadata::apply_metadata(&metadata::MetadataInput {
            file_path: &song_path,
            cover_path: &dir.join("cover.jpg"),
            title: &song.name,
            artist: &song.artistes,
            album: &album.name,
            album_artist: &album.artistes,
            track_number: song.track_number,
            comment: album.intro.as_deref(),
            lyrics_path: &lyrics_path,
        });
        match result {
            Ok(final_path) => {
                db::set_song_metadata_filled(conn, song.cid, &final_path.to_string_lossy())?;
            }
            Err(e) => eprintln!("写入元数据失败：{}：{e:#}", song.name),
        }
    }
    println!("歌曲下载完成");
    Ok(())
}

fn fill_metadata_for_existing(conn: &rusqlite::Connection, save_path: &str, replacement: &str) -> Result<()> {
    let songs = db::songs_needing_metadata(conn)?;
    if songs.is_empty() {
        println!("无需回填元数据");
        return Ok(());
    }
    println!("共有{}首歌曲待回填元数据", songs.len());
    for (i, song) in songs.iter().enumerate() {
        let Some(album) = db::album_by_cid(conn, song.album_cid)? else {
            continue;
        };
        let dir = album_dir(save_path, &album);
        let safe_name = sanitize_filename(&song.name, replacement);
        // 优先用库里的 filePath，失效则按常见扩展名找
        let candidate = song
            .file_path
            .as_ref()
            .filter(|p| Path::new(p).exists())
            .cloned()
            .map(PathBuf::from)
            .or_else(|| {
                ["flac", "wav", "mp3"]
                    .iter()
                    .map(|ext| dir.join(format!("{safe_name}.{ext}")))
                    .find(|p| p.exists())
            });
        let Some(candidate) = candidate else {
            eprintln!("找不到文件，跳过：{}", song.name);
            continue;
        };
        println!("正在回填元数据：{}，{}/{}", song.name, i + 1, songs.len());
        let lyrics_path = dir.join(format!("{safe_name}.lrc"));
        let result = metadata::apply_metadata(&metadata::MetadataInput {
            file_path: &candidate,
            cover_path: &dir.join("cover.jpg"),
            title: &song.name,
            artist: &song.artistes,
            album: &album.name,
            album_artist: &album.artistes,
            track_number: song.track_number,
            comment: album.intro.as_deref(),
            lyrics_path: &lyrics_path,
        });
        match result {
            Ok(final_path) => {
                db::set_song_metadata_filled(conn, song.cid, &final_path.to_string_lossy())?;
            }
            Err(e) => eprintln!("写入元数据失败：{}：{e:#}", song.name),
        }
    }
    println!("元数据回填完成");
    Ok(())
}
