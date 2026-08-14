use anyhow::{Context, Result};
use rusqlite::Connection;

// ponytail: DDL 与旧 TS 版（TypeORM synchronize 生成）逐字一致，保证旧 database.sqlite 直接续用
const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS "album" ("cid" integer PRIMARY KEY NOT NULL, "name" varchar NOT NULL, "coverUrl" varchar NOT NULL, "isCoverDownloaded" boolean NOT NULL DEFAULT (0), "artistes" varchar NOT NULL, "intro" varchar, "createdAt" datetime NOT NULL DEFAULT (datetime('now')), "updatedAt" datetime NOT NULL DEFAULT (datetime('now')), "belong" varchar, "coverDeUrl" varchar, "isCoverDeDownloaded" boolean NOT NULL DEFAULT (0));
CREATE TABLE IF NOT EXISTS "song" ("cid" integer PRIMARY KEY NOT NULL, "name" varchar NOT NULL, "albumCid" integer NOT NULL, "sourceUrl" varchar, "lyricUrl" varchar, "isDownloaded" boolean NOT NULL DEFAULT (0), "filePath" varchar, "isMetadataFilled" boolean NOT NULL DEFAULT (0), "artistes" varchar NOT NULL, "trackNumber" integer, "createdAt" datetime NOT NULL DEFAULT (datetime('now')), "updatedAt" datetime NOT NULL DEFAULT (datetime('now')));
"#;

#[derive(Debug, Clone)]
pub struct Album {
    pub cid: i64,
    pub name: String,
    pub cover_url: String,
    pub is_cover_downloaded: bool,
    pub artistes: String,
    pub intro: Option<String>,
    pub belong: Option<String>,
    pub cover_de_url: Option<String>,
    pub is_cover_de_downloaded: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // is_downloaded / is_metadata_filled 只在 SQL 条件里用到，结构体字段留作完整性
pub struct Song {
    pub cid: i64,
    pub name: String,
    pub album_cid: i64,
    pub source_url: Option<String>,
    pub lyric_url: Option<String>,
    pub is_downloaded: bool,
    pub file_path: Option<String>,
    pub is_metadata_filled: bool,
    pub artistes: String,
    pub track_number: Option<i64>,
}

fn row_to_album(row: &rusqlite::Row) -> rusqlite::Result<Album> {
    Ok(Album {
        cid: row.get(0)?,
        name: row.get(1)?,
        cover_url: row.get(2)?,
        is_cover_downloaded: row.get::<_, i64>(3)? != 0,
        artistes: row.get(4)?,
        intro: row.get(5)?,
        belong: row.get(6)?,
        cover_de_url: row.get(7)?,
        is_cover_de_downloaded: row.get::<_, i64>(8)? != 0,
    })
}

fn row_to_song(row: &rusqlite::Row) -> rusqlite::Result<Song> {
    Ok(Song {
        cid: row.get(0)?,
        name: row.get(1)?,
        album_cid: row.get(2)?,
        source_url: row.get(3)?,
        lyric_url: row.get(4)?,
        is_downloaded: row.get::<_, i64>(5)? != 0,
        file_path: row.get(6)?,
        is_metadata_filled: row.get::<_, i64>(7)? != 0,
        artistes: row.get(8)?,
        track_number: row.get(9)?,
    })
}

const ALBUM_COLS: &str =
    "cid, name, coverUrl, isCoverDownloaded, artistes, intro, belong, coverDeUrl, isCoverDeDownloaded";
const SONG_COLS: &str =
    "cid, name, albumCid, sourceUrl, lyricUrl, isDownloaded, filePath, isMetadataFilled, artistes, trackNumber";

pub fn open() -> Result<Connection> {
    let conn = Connection::open("database.sqlite").context("打开 database.sqlite 失败")?;
    conn.execute_batch(DDL)?;
    Ok(conn)
}

pub fn album_exists(conn: &Connection, cid: i64) -> Result<bool> {
    Ok(conn.query_row("SELECT 1 FROM album WHERE cid = ?1", [cid], |_| Ok(()))
        .is_ok())
}

pub fn song_exists(conn: &Connection, cid: i64) -> Result<bool> {
    Ok(conn.query_row("SELECT 1 FROM song WHERE cid = ?1", [cid], |_| Ok(()))
        .is_ok())
}

pub fn insert_album(
    conn: &Connection,
    cid: i64,
    name: &str,
    cover_url: &str,
    artistes: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO album (cid, name, coverUrl, artistes) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![cid, name, cover_url, artistes],
    )?;
    Ok(())
}

pub fn insert_song(
    conn: &Connection,
    cid: i64,
    name: &str,
    album_cid: i64,
    artistes: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO song (cid, name, albumCid, artistes) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![cid, name, album_cid, artistes],
    )?;
    Ok(())
}

pub fn albums_missing_intro(conn: &Connection) -> Result<Vec<Album>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ALBUM_COLS} FROM album WHERE intro IS NULL"
    ))?;
    let rows = stmt.query_map([], row_to_album)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("读取专辑列表失败")
}

pub fn update_album_detail(
    conn: &Connection,
    cid: i64,
    intro: &str,
    belong: &str,
    cover_de_url: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE album SET intro = ?2, belong = ?3, coverDeUrl = ?4, updatedAt = datetime('now') WHERE cid = ?1",
        rusqlite::params![cid, intro, belong, cover_de_url],
    )?;
    Ok(())
}

pub fn set_track_number(conn: &Connection, song_cid: i64, track: i64) -> Result<()> {
    conn.execute(
        "UPDATE song SET trackNumber = ?2, updatedAt = datetime('now') WHERE cid = ?1",
        rusqlite::params![song_cid, track],
    )?;
    Ok(())
}

pub fn all_albums(conn: &Connection) -> Result<Vec<Album>> {
    let mut stmt = conn.prepare(&format!("SELECT {ALBUM_COLS} FROM album"))?;
    let rows = stmt.query_map([], row_to_album)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("读取专辑列表失败")
}

pub fn set_cover_downloaded(conn: &Connection, cid: i64, is_de: bool) -> Result<()> {
    let sql = if is_de {
        "UPDATE album SET isCoverDeDownloaded = 1, updatedAt = datetime('now') WHERE cid = ?1"
    } else {
        "UPDATE album SET isCoverDownloaded = 1, updatedAt = datetime('now') WHERE cid = ?1"
    };
    conn.execute(sql, [cid])?;
    Ok(())
}

pub fn songs_missing_source(conn: &Connection) -> Result<Vec<Song>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SONG_COLS} FROM song WHERE sourceUrl IS NULL"
    ))?;
    let rows = stmt.query_map([], row_to_song)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("读取歌曲列表失败")
}

pub fn update_song_source(conn: &Connection, cid: i64, source_url: &str, lyric_url: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE song SET sourceUrl = ?2, lyricUrl = ?3, updatedAt = datetime('now') WHERE cid = ?1",
        rusqlite::params![cid, source_url, lyric_url],
    )?;
    Ok(())
}

pub fn songs_not_downloaded(conn: &Connection) -> Result<Vec<Song>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SONG_COLS} FROM song WHERE isDownloaded = 0"
    ))?;
    let rows = stmt.query_map([], row_to_song)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("读取歌曲列表失败")
}

pub fn songs_needing_metadata(conn: &Connection) -> Result<Vec<Song>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SONG_COLS} FROM song WHERE isDownloaded = 1 AND isMetadataFilled = 0"
    ))?;
    let rows = stmt.query_map([], row_to_song)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("读取歌曲列表失败")
}

pub fn album_by_cid(conn: &Connection, cid: i64) -> Result<Option<Album>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ALBUM_COLS} FROM album WHERE cid = ?1"
    ))?;
    let mut rows = stmt.query_map([cid], row_to_album)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn set_song_downloaded(conn: &Connection, cid: i64, file_path: &str) -> Result<()> {
    conn.execute(
        "UPDATE song SET isDownloaded = 1, filePath = ?2, updatedAt = datetime('now') WHERE cid = ?1",
        rusqlite::params![cid, file_path],
    )?;
    Ok(())
}

pub fn set_song_metadata_filled(conn: &Connection, cid: i64, file_path: &str) -> Result<()> {
    conn.execute(
        "UPDATE song SET isMetadataFilled = 1, filePath = ?2, updatedAt = datetime('now') WHERE cid = ?1",
        rusqlite::params![cid, file_path],
    )?;
    Ok(())
}
