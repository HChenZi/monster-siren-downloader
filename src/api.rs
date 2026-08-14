use anyhow::{Context, Result};
use serde::Deserialize;
use std::fmt;

const BASE_URL: &str = "https://monster-siren.hypergryph.com/api";

// API 把 cid 以字符串返回，统一转成 i64，与 SQLite 整型主键一致
fn cid_from_str<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct CidVisitor;
    impl serde::de::Visitor<'_> for CidVisitor {
        type Value = i64;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("cid as string or number")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<i64, E> {
            v.parse().map_err(serde::de::Error::custom)
        }
        fn visit_i64<E>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(serde::de::Error::custom)
        }
    }
    deserializer.deserialize_any(CidVisitor)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumListItem {
    #[serde(deserialize_with = "cid_from_str")]
    pub cid: i64,
    pub name: String,
    pub cover_url: String,
    pub artistes: Vec<String>,
}

#[derive(Deserialize)]
pub struct SongsResp {
    pub list: Vec<SongListItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongListItem {
    #[serde(deserialize_with = "cid_from_str")]
    pub cid: i64,
    pub name: String,
    #[serde(deserialize_with = "cid_from_str")]
    pub album_cid: i64,
    pub artists: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongDetail {
    pub source_url: String,
    pub lyric_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumDetail {
    pub intro: Option<String>,
    pub belong: Option<String>,
    pub cover_de_url: Option<String>,
    pub songs: Option<Vec<AlbumDetailSong>>,
}

#[derive(Deserialize)]
pub struct AlbumDetailSong {
    #[serde(deserialize_with = "cid_from_str")]
    pub cid: i64,
}

// 各端点统一返回 { code, msg, data } 信封，只取 data
#[derive(Deserialize)]
struct Envelope<T> {
    pub data: Option<T>,
}

async fn get_data<T: serde::de::DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    let resp: Envelope<T> = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("请求失败：{url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP 错误：{url}"))?
        .json()
        .await
        .with_context(|| format!("解析响应失败：{url}"))?;
    resp.data.context(format!("响应无 data 字段：{url}"))
}

pub async fn get_albums(client: &reqwest::Client) -> Result<Vec<AlbumListItem>> {
    get_data(client, &format!("{BASE_URL}/albums")).await
}

pub async fn get_songs(client: &reqwest::Client) -> Result<SongsResp> {
    get_data(client, &format!("{BASE_URL}/songs")).await
}

pub async fn get_song_detail(client: &reqwest::Client, cid: i64) -> Result<SongDetail> {
    get_data(client, &format!("{BASE_URL}/song/{cid}")).await
}

pub async fn get_album_detail(client: &reqwest::Client, cid: i64) -> Result<AlbumDetail> {
    get_data(client, &format!("{BASE_URL}/album/{cid}/detail")).await
}
