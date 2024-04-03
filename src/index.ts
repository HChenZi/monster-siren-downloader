import { exit } from "process";
import "dotenv/config";
import { getAlbums, getSongDetail, getSongs } from "./api.js";
import { AppDataSource } from "./data-source.js";
import downloadFile from "./downloadFile.js";
import { Album } from "./entity/Album.js";
import { Song } from "./entity/Song.js";
import { IsNull } from "typeorm";
import filenamify from "filenamify";

const SAVE_PATH = process.env.SAVE_PATH;
if (!SAVE_PATH) {
  console.error("请设置保存路径");
  exit();
}

const replacement = process.env.REPLACE_CHAR || "_";

const main = async () => {
  AppDataSource.initialize()
    .then(async () => {
      const albums: any[] = await getAlbums();
      const songs: any[] = (await getSongs()).list;
      console.log(`官网共有${albums.length}个专辑，${songs.length}首歌曲`);
      await downloadAlbums(albums);
      await downloadSong(songs);
    })
    .catch((error) => console.log(error));
};

main();

const downloadAlbums = async (albums) => {
  // 获取所有专辑并存入数据库
  for (const album of albums) {
    if (
      await AppDataSource.manager.findOne(Album, { where: { cid: album.cid } })
    ) {
      continue;
    }
    const albumEntity = new Album();
    albumEntity.cid = album.cid;
    albumEntity.name = album.name;
    albumEntity.coverUrl = album.coverUrl;
    albumEntity.artistes = album.artistes.join(",");
    await AppDataSource.manager.save(albumEntity);
  }
  // 遍历数据库，如果有未下载封面的专辑，下载专辑封面
  const albumsToDownload = await AppDataSource.manager.find(Album, {
    where: {
      isCoverDownloaded: false,
    },
  });
  console.log(`共有${albumsToDownload.length}个专辑封面未下载`);
  let albumCount = 0;
  for (const album of albumsToDownload) {
    console.log(
      `正在下载专辑封面：${album.name}, 当前进度：${++albumCount}/${albumsToDownload.length}`,
    );
    await downloadFile(album.coverUrl, `${SAVE_PATH}/${album.name}/cover.jpg`);
    album.isCoverDownloaded = true;
    await AppDataSource.manager.save(album);
  }
  console.log("专辑封面下载完成");
};

const downloadSong = async (songs) => {
  // 获取所有歌曲并存入数据库
  for (const song of songs) {
    if (
      await AppDataSource.manager.findOne(Song, { where: { cid: song.cid } })
    ) {
      continue;
    }
    const songEntity = new Song();
    songEntity.cid = song.cid;
    songEntity.name = song.name;
    songEntity.albumCid = song.albumCid;
    songEntity.artistes = song.artists.join(",");
    await AppDataSource.manager.save(songEntity);
  }

  // 遍历数据库，如果有未获取下载地址的歌曲，获取歌曲详情
  const songsToFetch = await AppDataSource.manager.find(Song, {
    where: {
      sourceUrl: IsNull(),
    },
  });
  console.log(`共有${songsToFetch.length}首歌曲未获取下载地址`);
  for (const song of songsToFetch) {
    console.log(
      `正在获取歌曲详情：${song.name}，当前进度：${songsToFetch.indexOf(song) + 1}/${songsToFetch.length}`,
    );
    const songDetail = await getSongDetail(song.cid);
    if (songDetail) {
      song.sourceUrl = songDetail.sourceUrl;
      song.lyricUrl = songDetail.lyricUrl;
    }
    await AppDataSource.manager.save(song);
  }

  // 遍历数据库，如果有未下载的歌曲，下载歌曲
  const songsToDownload = await AppDataSource.manager.find(Song, {
    where: {
      isDownloaded: false,
    },
  });
  console.log(`共有${songsToDownload.length}首歌曲未下载`);
  let songCount = 0;
  for (const song of songsToDownload) {
    console.log(
      `正在下载歌曲：${song.name}，当前进度：${++songCount}/${songsToDownload.length}`,
    );
    const albumName = (
      await AppDataSource.manager.findOne(Album, {
        where: { cid: song.albumCid },
      })
    ).name;
    // 下载歌曲
    const ext = song.sourceUrl.split(".").pop();
    await downloadFile(
      song.sourceUrl,
      `${SAVE_PATH}/${albumName}/${filenamify(song.name, { replacement })}.${ext}`,
    );
    // 下载歌词
    if (song.lyricUrl) {
      await downloadFile(
        song.lyricUrl,
        `${SAVE_PATH}/${albumName}/${filenamify(song.name, { replacement })}.lrc`,
      );
    }
    song.isDownloaded = true;
    await AppDataSource.manager.save(song);
  }
  console.log("歌曲下载完成");
};
