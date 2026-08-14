import { exit } from "process";
import fs from "node:fs";
import path from "node:path";
import "dotenv/config";
import { getAlbumDetail, getAlbums, getSongDetail, getSongs } from "./api.js";
import { AppDataSource } from "./data-source.js";
import downloadFile from "./downloadFile.js";
import { Album } from "./entity/Album.js";
import { Song } from "./entity/Song.js";
import { IsNull } from "typeorm";
import filenamify from "filenamify";
import { applyMetadata } from "./metadata.js";

const SAVE_PATH = process.env.SAVE_PATH;
if (!SAVE_PATH) {
  console.error("请设置保存路径");
  exit();
}

const replacement = process.env.REPLACE_CHAR || "_";
const DEFAULT_GAME = "arknights";

// API 现在把 cid 以字符串返回，统一转成数字，避免与 SQLite 整型主键不一致
const num = (value: any): number => Number(value);

const extFromUrl = (url: string | null | undefined): string => {
  if (!url) return "";
  try {
    const pathname = new URL(url).pathname;
    return path.extname(pathname).slice(1).toLowerCase();
  } catch {
    return (url.split(".").pop() ?? "").toLowerCase();
  }
};

// 输出目录按游戏分：<SAVE_PATH>/<belong>/<专辑名>，为多游戏预留
const albumDir = (album: Album): string =>
  path.join(SAVE_PATH, album.belong || DEFAULT_GAME, album.name);

const main = async () => {
  AppDataSource.initialize()
    .then(async () => {
      const albums: any[] = await getAlbums();
      const songs: any[] = (await getSongs()).list;
      console.log(`官网共有${albums.length}个专辑，${songs.length}首歌曲`);
      await ingestAlbums(albums);
      await insertSongs(songs);
      await fetchAlbumDetails();
      await downloadCovers();
      await fetchSongDetails();
      await downloadAndTagSongs();
      await fillMetadataForExisting();
    })
    .catch((error) => console.log(error));
};

main();

const ingestAlbums = async (albums) => {
  // 获取所有专辑并存入数据库（只存元数据，封面延后到 fetchAlbumDetails 之后统一下载）
  for (const album of albums) {
    if (
      await AppDataSource.manager.findOne(Album, { where: { cid: num(album.cid) } })
    ) {
      continue;
    }
    const albumEntity = new Album();
    albumEntity.cid = num(album.cid);
    albumEntity.name = album.name;
    albumEntity.coverUrl = album.coverUrl;
    albumEntity.artistes = album.artistes.join(",");
    await AppDataSource.manager.save(albumEntity);
  }
};

const insertSongs = async (songs) => {
  for (const song of songs) {
    if (
      await AppDataSource.manager.findOne(Song, { where: { cid: num(song.cid) } })
    ) {
      continue;
    }
    const songEntity = new Song();
    songEntity.cid = num(song.cid);
    songEntity.name = song.name;
    songEntity.albumCid = num(song.albumCid);
    songEntity.artistes = song.artists.join(",");
    await AppDataSource.manager.save(songEntity);
  }
};

const fetchSongDetails = async () => {
  const songsToFetch = await AppDataSource.manager.find(Song, {
    where: {
      sourceUrl: IsNull(),
    },
  });
  console.log(`共有${songsToFetch.length}首歌曲未获取下载地址`);
  let fetchCount = 0;
  for (const song of songsToFetch) {
    fetchCount++;
    console.log(
      `正在获取歌曲详情：${song.name}，当前进度：${fetchCount}/${songsToFetch.length}`,
    );
    try {
      const songDetail = await getSongDetail(song.cid);
      if (songDetail) {
        song.sourceUrl = songDetail.sourceUrl;
        song.lyricUrl = songDetail.lyricUrl;
        await AppDataSource.manager.save(song);
      }
    } catch (error) {
      console.error(`获取歌曲详情失败：${song.name}`, error);
    }
  }
};

const downloadAndTagSongs = async () => {
  const songsToDownload = await AppDataSource.manager.find(Song, {
    where: {
      isDownloaded: false,
    },
  });
  console.log(`共有${songsToDownload.length}首歌曲未下载`);
  let songCount = 0;
  for (const song of songsToDownload) {
    songCount++;
    console.log(
      `正在下载歌曲：${song.name}，当前进度：${songCount}/${songsToDownload.length}`,
    );
    try {
      const album = await AppDataSource.manager.findOne(Album, {
        where: { cid: song.albumCid },
      });
      if (!album) {
        console.warn(`找不到所属专辑，跳过：${song.name}`);
        continue;
      }
      if (!song.sourceUrl) {
        console.warn(`无下载地址，跳过：${song.name}`);
        continue;
      }
      const dir = albumDir(album);
      const safeName = filenamify(song.name, { replacement });
      const ext = extFromUrl(song.sourceUrl);
      const songPath = path.join(dir, `${safeName}.${ext}`);
      const lyricsPath = path.join(dir, `${safeName}.lrc`);

      // 下载歌曲
      await downloadFile(song.sourceUrl, songPath);
      // 下载歌词
      if (song.lyricUrl) {
        await downloadFile(song.lyricUrl, lyricsPath);
      }
      song.filePath = songPath;
      song.isDownloaded = true;
      await AppDataSource.manager.save(song);

      try {
        const { finalPath } = await applyMetadata({
          filePath: songPath,
          coverPath: path.join(dir, "cover.jpg"),
          title: song.name,
          artist: song.artistes,
          album: album.name,
          albumArtist: album.artistes ?? song.artistes,
          trackNumber: song.trackNumber ?? undefined,
          comment: album.intro ?? undefined,
          lyricsPath,
        });
        song.filePath = finalPath;
        song.isMetadataFilled = true;
        await AppDataSource.manager.save(song);
      } catch (error) {
        console.error(`写入元数据失败：${song.name}`, error);
      }
    } catch (error) {
      console.error(`下载歌曲失败：${song.name}`, error);
    }
  }
  console.log("歌曲下载完成");
};

const fillMetadataForExisting = async () => {
  const songs = await AppDataSource.manager.find(Song, {
    where: { isDownloaded: true, isMetadataFilled: false },
  });
  if (songs.length === 0) {
    console.log("无需回填元数据");
    return;
  }
  console.log(`共有${songs.length}首歌曲待回填元数据`);
  let count = 0;
  for (const song of songs) {
    count++;
    const album = await AppDataSource.manager.findOne(Album, {
      where: { cid: song.albumCid },
    });
    if (!album) continue;
    const dir = albumDir(album);
    let candidate: string | undefined =
      song.filePath && fs.existsSync(song.filePath) ? song.filePath : undefined;
    if (!candidate) {
      const safeName = filenamify(song.name, { replacement });
      const candidates = ["flac", "wav", "mp3"].map(
        (ext) => path.join(dir, `${safeName}.${ext}`),
      );
      candidate = candidates.find((p) => fs.existsSync(p));
    }
    if (!candidate) {
      console.warn(`找不到文件，跳过：${song.name}`);
      continue;
    }
    console.log(`正在回填元数据：${song.name}，${count}/${songs.length}`);
    try {
      const { finalPath } = await applyMetadata({
        filePath: candidate,
        coverPath: path.join(dir, "cover.jpg"),
        title: song.name,
        artist: song.artistes,
        album: album.name,
        albumArtist: album.artistes,
        trackNumber: song.trackNumber ?? undefined,
        comment: album.intro ?? undefined,
        lyricsPath: path.join(dir, `${filenamify(song.name, { replacement })}.lrc`),
      });
      song.filePath = finalPath;
      song.isMetadataFilled = true;
      await AppDataSource.manager.save(song);
    } catch (error) {
      console.error(`写入元数据失败：${song.name}`, error);
    }
  }
  console.log("元数据回填完成");
};

const fetchAlbumDetails = async () => {
  const albumsToFetch = await AppDataSource.manager.find(Album, {
    where: { intro: IsNull() },
  });
  if (albumsToFetch.length === 0) return;
  console.log(`共有${albumsToFetch.length}个专辑待拉取详情`);
  let count = 0;
  for (const album of albumsToFetch) {
    count++;
    console.log(`正在拉取专辑详情：${album.name}，${count}/${albumsToFetch.length}`);
    try {
      const detail: any = await getAlbumDetail(album.cid);
      album.intro = detail?.intro ?? "";
      album.belong = detail?.belong ?? DEFAULT_GAME;
      album.coverDeUrl = detail?.coverDeUrl ?? null;
      await AppDataSource.manager.save(album);
      const songs = Array.isArray(detail?.songs) ? detail.songs : [];
      for (let i = 0; i < songs.length; i++) {
        const songRow = await AppDataSource.manager.findOne(Song, {
          where: { cid: num(songs[i].cid) },
        });
        if (!songRow) continue;
        songRow.trackNumber = i + 1;
        await AppDataSource.manager.save(songRow);
      }
    } catch (error) {
      console.error(`拉取专辑详情失败：${album.name}`, error);
    }
  }
  console.log("专辑详情拉取完成");
};

const downloadCovers = async () => {
  const albums = await AppDataSource.manager.find(Album);
  const needCover = albums.filter((a) => !a.isCoverDownloaded && a.coverUrl);
  const needCoverDe = albums.filter(
    (a) => a.coverDeUrl && !a.isCoverDeDownloaded,
  );
  console.log(
    `共有${needCover.length}个专辑封面未下载，${needCoverDe.length}个高清封面未下载`,
  );
  const total = needCover.length + needCoverDe.length;
  let count = 0;
  for (const album of needCover) {
    try {
      await downloadFile(album.coverUrl, path.join(albumDir(album), "cover.jpg"));
      album.isCoverDownloaded = true;
      await AppDataSource.manager.save(album);
      console.log(`封面下载完成：${album.name}，${++count}/${total}`);
    } catch (error) {
      console.error(`封面下载失败：${album.name}`, error);
    }
  }
  for (const album of needCoverDe) {
    try {
      const ext = extFromUrl(album.coverDeUrl) || "jpg";
      await downloadFile(
        album.coverDeUrl as string,
        path.join(albumDir(album), `cover_de.${ext}`),
      );
      album.isCoverDeDownloaded = true;
      await AppDataSource.manager.save(album);
      console.log(`高清封面下载完成：${album.name}，${++count}/${total}`);
    } catch (error) {
      console.error(`高清封面下载失败：${album.name}`, error);
    }
  }
  console.log("封面下载完成");
};
