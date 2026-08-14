# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Purpose

CLI scraper that downloads every track, album cover, and lyric file published on the Arknights *Monster Siren Records* site (`https://monster-siren.hypergryph.com`). The pipeline is incremental and resumable: progress is persisted in a local SQLite database so re-running picks up where the previous run stopped.

## Commands

```bash
npm install          # Install dependencies
npm run start        # Run the downloader (node --loader ts-node/esm src/index.ts)
```

There is no build, lint, or test setup. TypeScript is executed directly via `ts-node/esm` — do not look for a `dist/` or `build/` step. The `typeorm` script (`typeorm-ts-node-esm`) is wired up for ad-hoc TypeORM CLI usage but no migrations are tracked (see schema note below).

### Required configuration

`.env` is read on startup:

- `SAVE_PATH` — output root (process exits if missing). Default in committed `.env` is `./`. Files are grouped under a per-game subfolder (`<SAVE_PATH>/<belong>/<albumName>/`), so Arknights content currently lands in `./arknights/` and future games get their own sibling directory.
- `REPLACE_CHAR` — replacement char for filesystem-illegal characters in song names (default `_`). Applied via `filenamify` only to song filenames, **not** to album folder names.

## Architecture

The whole program is `src/index.ts` plus a few small helpers. The key idea is a **staged idempotent pipeline** backed by SQLite, where each stage writes a flag that lets the next run skip completed work.

```
Monster Siren API ──► api.ts ──► index.ts ──► TypeORM/SQLite ──► downloadFile.ts ──► <SAVE_PATH>/<belong>/<albumName>/
```

### Stage flow (in `src/index.ts`)

The pipeline is **front-loaded with all metadata fetching** so that by the time we start downloading audio, every per-song fact (title/artist/album/album_artist/track/intro) is already in SQLite. This ordering matters: stage 6 inlines `applyMetadata()` right after each download, and it must see populated `Song.trackNumber` / `Album.intro` to produce complete tags.

1. **Albums metadata.** `ingestAlbums()` ingests `getAlbums()` into `Album` (PK `cid`, `artistes` joined with comma). Covers are downloaded later, after album details reveal `belong` and `coverDeUrl`.
2. **Songs metadata.** `insertSongs()` ingests `getSongs().list` into `Song` (cid/name/albumCid/artistes only). The flat `/songs` endpoint does not carry track order — that's filled in stage 3.
3. **Album detail backfill.** `fetchAlbumDetails()` finds `Album` rows with `intro IS NULL` and calls `/api/album/{cid}/detail`. The response carries `intro` (album description → `Album.intro`; empty string is stored as the "fetched but no description" sentinel so it's not retried), `belong` (game id → `Album.belong`, defaults to `arknights`), `coverDeUrl` (high-res/alt cover) and an ordered `songs[]` array used to write `Song.trackNumber = index + 1` for every song already in the local DB.
4. **Covers.** `downloadCovers()` downloads `cover.jpg` (from `coverUrl`, guarded by `isCoverDownloaded`) and `cover_de.<ext>` (from `coverDeUrl`, guarded by `isCoverDeDownloaded`) into `<SAVE_PATH>/<belong>/<album.name>/`.
5. **Song detail enrichment.** `fetchSongDetails()` hydrates rows with `sourceUrl IS NULL` via `getSongDetail(cid)` to fetch `sourceUrl` and (optionally) `lyricUrl`.
6. **Song + lyric → disk + inline metadata.** `downloadAndTagSongs()` walks rows where `isDownloaded=false`, downloads the audio to `<SAVE_PATH>/<belong>/<album.name>/<sanitizedSongName>.<ext>` (extension inferred from `sourceUrl`) and any lyric file alongside as `.lrc`, records `filePath`, flips `isDownloaded`. Immediately after, `applyMetadata()` is invoked (see stage 7 dispatch); on success `isMetadataFilled` is flipped, on failure the row is left for stage 7 to retry.
7. **Metadata fill / format dispatch.** `fillMetadataForExisting()` scans `isDownloaded=true AND isMetadataFilled=false` (covers stage 6 failures plus legacy DB rows). `src/metadata.ts` dispatches by extension:
   - `.wav` → spawn `ffmpeg-static` to encode to FLAC, embedding `title` / `artist` / `album` / `album_artist` / `track` / `comment` (= `album.intro`) / `lyrics` (read from the sibling `.lrc`) Vorbis comments and a `attached_pic` stream from `cover.jpg`; the original `.wav` is unlinked and `Song.filePath` is updated to the new `.flac`.
   - `.flac` → spawn `ffmpeg-static` with `-c copy -map_metadata -1 -metadata ...` to **rewrite tags in-place via a temp file + rename**, no re-encoding. Used for legacy FLACs that need newly-fetched fields like `track` / `comment` re-stamped.
   - `.mp3` → `music-metadata.parseFile` checks if title/artist/album/cover are already present; if any are missing, `node-id3.update` writes `TIT2`/`TPE1`/`TALB`/`TPE2`/`TRCK`/`COMM`/`USLT` plus an `APIC` cover frame.
   - other extensions → no-op.

### Persistence

- `src/data-source.ts` configures TypeORM with `type: "sqlite"`, `database: "database.sqlite"` (project root, gitignored) and **`synchronize: true`** — the schema is auto-derived from entity decorators on every startup. There are no migrations; do not add any unless you also turn off `synchronize`.
- `src/entity/Album.ts` and `src/entity/Song.ts` carry the download-state flags (`isCoverDownloaded`, `isCoverDeDownloaded`, `isDownloaded`, `isMetadataFilled`) plus `sourceUrl` / `lyricUrl` (populated lazily in stage 5), `belong` / `coverDeUrl` / `intro` (populated in stage 3) and `filePath` (final on-disk path, may flip from `.wav` to `.flac` after stage 7). Deleting `database.sqlite` is the supported "redo from scratch" reset (the audio files on disk will be re-downloaded if missing, but already-present files will be silently overwritten because `downloadFile.ts` does not check existence).

### API client

`src/api.ts` wraps four unauthenticated endpoints under `https://monster-siren.hypergryph.com/api`: `/albums`, `/songs`, `/song/:cid`, `/album/:cid/detail`. Responses are typed as `any` and unwrapped via `res.data`. Note that the API returns `cid`/`albumCid` as **strings**; `src/index.ts` coerces them via `Number()` at ingestion.

### Download helper

`src/downloadFile.ts` creates the destination directory if needed, then streams the response body to disk using `node:stream/pipeline`. It checks `res.ok` and retries up to 3 times with linear backoff on failure. There is still no resume and no progress bar — keep this in mind when extending.

## Conventions specific to this repo

- **ESM with TS source.** `package.json` sets `"type": "module"`; relative imports inside `src/` use `.js` suffixes (e.g. `import ... from "./api.js"`) even though the files are `.ts`. This is required by the `ts-node/esm` loader — do not rewrite them to `.ts`.
- **No types for API payloads.** External JSON is treated as `any` and field access (`album.cid`, `song.artists`) is implicit. Adding stronger typing is welcome but is currently not how the code reads.
- **Album folder names are not sanitized**, only song filenames are. If an album name contains characters illegal on the target filesystem, the directory creation in `downloadFile.ts` will fail.
