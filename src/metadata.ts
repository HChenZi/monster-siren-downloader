import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import ffmpegPath from "ffmpeg-static";
import NodeID3 from "node-id3";

export interface MetadataInput {
  filePath: string;
  coverPath?: string;
  title: string;
  artist: string;
  album: string;
  albumArtist: string;
  trackNumber?: number;
  totalTracks?: number;
  comment?: string;
  lyricsPath?: string;
}

export const applyMetadata = async (
  input: MetadataInput,
): Promise<{ finalPath: string; converted: boolean }> => {
  const ext = path.extname(input.filePath).toLowerCase();
  if (ext === ".wav") {
    const finalPath = await transcodeWavToFlac(input);
    return { finalPath, converted: true };
  }
  if (ext === ".mp3") {
    await ensureMp3Tags(input);
    return { finalPath: input.filePath, converted: false };
  }
  if (ext === ".flac") {
    await rewriteFlacTags(input);
    return { finalPath: input.filePath, converted: false };
  }
  return { finalPath: input.filePath, converted: false };
};

const rewriteFlacTags = async (input: MetadataInput): Promise<void> => {
  if (!ffmpegPath) {
    throw new Error("ffmpeg-static did not provide a binary path");
  }
  const tmpPath = `${input.filePath}.tmp.flac`;
  const hasCover = !!input.coverPath && fs.existsSync(input.coverPath);

  const args: string[] = ["-y", "-i", input.filePath];
  if (hasCover) {
    args.push("-i", input.coverPath as string);
    args.push(
      "-map", "0:a",
      "-map", "1:v",
      "-c:a", "copy",
      "-c:v", "copy",
      "-disposition:v", "attached_pic",
    );
  } else {
    args.push("-map", "0", "-c", "copy");
  }
  args.push("-map_metadata", "-1");
  args.push(
    "-metadata", `title=${input.title}`,
    "-metadata", `artist=${input.artist}`,
    "-metadata", `album=${input.album}`,
    "-metadata", `album_artist=${input.albumArtist}`,
  );
  if (typeof input.trackNumber === "number") {
    const trackValue = input.totalTracks
      ? `${input.trackNumber}/${input.totalTracks}`
      : String(input.trackNumber);
    args.push("-metadata", `track=${trackValue}`);
  }
  if (input.comment) {
    args.push("-metadata", `comment=${input.comment}`);
  }
  const lyrics = readLyricsText(input.lyricsPath);
  if (lyrics) {
    args.push("-metadata", `lyrics=${lyrics}`);
  }
  args.push(tmpPath);

  await runFfmpeg(args);
  fs.renameSync(tmpPath, input.filePath);
};

const transcodeWavToFlac = async (input: MetadataInput): Promise<string> => {
  if (!ffmpegPath) {
    throw new Error("ffmpeg-static did not provide a binary path");
  }
  const flacPath = input.filePath.replace(/\.wav$/i, ".flac");
  const hasCover = !!input.coverPath && fs.existsSync(input.coverPath);

  const args: string[] = ["-y", "-i", input.filePath];
  if (hasCover) {
    args.push("-i", input.coverPath as string);
    args.push(
      "-map", "0:a",
      "-map", "1:v",
      "-c:v", "copy",
      "-disposition:v", "attached_pic",
    );
  }
  args.push(
    "-metadata", `title=${input.title}`,
    "-metadata", `artist=${input.artist}`,
    "-metadata", `album=${input.album}`,
    "-metadata", `album_artist=${input.albumArtist}`,
  );
  if (typeof input.trackNumber === "number") {
    const trackValue = input.totalTracks
      ? `${input.trackNumber}/${input.totalTracks}`
      : String(input.trackNumber);
    args.push("-metadata", `track=${trackValue}`);
  }
  if (input.comment) {
    args.push("-metadata", `comment=${input.comment}`);
  }
  const lyrics = readLyricsText(input.lyricsPath);
  if (lyrics) {
    args.push("-metadata", `lyrics=${lyrics}`);
  }
  args.push(flacPath);

  await runFfmpeg(args);
  fs.unlinkSync(input.filePath);
  return flacPath;
};

const readLyricsText = (lyricsPath?: string): string | undefined => {
  if (!lyricsPath) return undefined;
  if (!fs.existsSync(lyricsPath)) return undefined;
  const text = fs.readFileSync(lyricsPath, "utf-8").trim();
  return text || undefined;
};

const runFfmpeg = (args: string[]): Promise<void> =>
  new Promise((resolve, reject) => {
    const proc = spawn(ffmpegPath as string, args, {
      stdio: ["ignore", "ignore", "pipe"],
    });
    let stderr = "";
    proc.stderr?.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    proc.on("error", reject);
    proc.on("close", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`ffmpeg exited ${code}: ${stderr.slice(-800)}`));
    });
  });

const ensureMp3Tags = async (input: MetadataInput): Promise<void> => {
  const tags: NodeID3.Tags = {
    title: input.title,
    artist: input.artist,
    album: input.album,
    performerInfo: input.albumArtist,
  };
  if (typeof input.trackNumber === "number") {
    tags.trackNumber = input.totalTracks
      ? `${input.trackNumber}/${input.totalTracks}`
      : String(input.trackNumber);
  }
  if (input.comment) {
    tags.comment = {
      language: "eng",
      text: input.comment,
    };
  }
  const lyricsText = readLyricsText(input.lyricsPath);
  if (lyricsText) {
    tags.unsynchronisedLyrics = {
      language: "eng",
      text: lyricsText,
    };
  }
  if (input.coverPath && fs.existsSync(input.coverPath)) {
    tags.image = {
      mime: "image/jpeg",
      type: { id: 3, name: "front cover" },
      description: "Cover",
      imageBuffer: fs.readFileSync(input.coverPath),
    };
  }
  const result = NodeID3.update(tags, input.filePath);
  if (result !== true) {
    throw new Error(`node-id3 failed: ${result}`);
  }
};
