# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Purpose

CLI scraper (Rust) that downloads every track, album cover, and lyric file published on the Arknights *Monster Siren Records* site (`https://monster-siren.hypergryph.com`). The pipeline is incremental and resumable: progress is persisted in a local SQLite database so re-running picks up where the previous run stopped.

## Commands

```bash
cargo run      # Build and run the downloader
cargo build    # Build only
```

System requirements: Rust toolchain, and `ffmpeg` on PATH (only used for WAV → FLAC transcoding; FLAC/MP3 tagging is done in-process via the `metaflac` / `id3` crates, no re-encoding).

### Required configuration

`.env` is read on startup (via `dotenvy`):

- `SAVE_PATH` — output root (process exits if missing). Default in committed `.env` is `./`. Files are grouped under a per-game subfolder (`<SAVE_PATH>/<belong>/<albumName>/`), so Arknights content currently lands in `./arknights/` and future games get their own sibling directory.
- `REPLACE_CHAR` — replacement char for filesystem-illegal characters in song names (default `_`). Applied by `sanitize_filename` in `src/main.rs` only to song filenames, **not** to album folder names.

## Architecture

Five modules in `src/`. The key idea is a **staged idempotent pipeline** backed by SQLite, where each stage writes a flag that lets the next run skip completed work.

```
Monster Siren API ──► api.rs ──► main.rs ──► rusqlite/SQLite ──► download.rs ──► <SAVE_PATH>/<belong>/<albumName>/
                                                                              └─► metadata.rs (metaflac/id3/ffmpeg)
```

### Stage flow (in `src/main.rs`)

The pipeline is **front-loaded with all metadata fetching** so that by the time we start downloading audio, every per-song fact (title/artist/album/album_artist/track/intro) is already in SQLite. This ordering matters: stage 6 inlines `apply_metadata()` right after each download, and it must see populated `trackNumber` / `Album.intro` to produce complete tags.

1. **Albums metadata.** `ingest_albums()` ingests `get_albums()` into the `album` table (PK `cid`, `artistes` joined with comma). Covers are downloaded later, after album details reveal `belong` and `coverDeUrl`.
2. **Songs metadata.** `insert_songs()` ingests `get_songs().list` into the `song` table (cid/name/albumCid/artistes only). The flat `/songs` endpoint does not carry track order — that's filled in stage 3.
3. **Album detail backfill.** `fetch_album_details()` finds `album` rows with `intro IS NULL` and calls `/api/album/{cid}/detail`. The response carries `intro` (album description → stored; empty string is the "fetched but no description" sentinel so it's not retried), `belong` (game id, defaults to `arknights`), `coverDeUrl` (high-res/alt cover) and an ordered `songs[]` array used to write `trackNumber = index + 1` for every song already in the local DB.
4. **Covers.** `download_covers()` downloads `cover.jpg` (from `coverUrl`, guarded by `isCoverDownloaded`) and `cover_de.<ext>` (from `coverDeUrl`, guarded by `isCoverDeDownloaded`) into `<SAVE_PATH>/<belong>/<album.name>/`.
5. **Song detail enrichment.** `fetch_song_details()` hydrates rows with `sourceUrl IS NULL` via `get_song_detail(cid)` to fetch `sourceUrl` and (optionally) `lyricUrl`.
6. **Song + lyric → disk + inline metadata.** `download_and_tag_songs()` walks rows where `isDownloaded = 0`, downloads the audio to `<SAVE_PATH>/<belong>/<album.name>/<sanitizedSongName>.<ext>` (extension inferred from `sourceUrl` via `ext_from_url`) and any lyric file alongside as `.lrc`, records `filePath`, flips `isDownloaded`. **The CDN `sourceUrl` is signed with an expiry** (the `/69FDD850/`-style path segment), so a URL stored in the DB from an old run 403s; on download failure the stage re-fetches `/song/:cid`, updates `sourceUrl`/`lyricUrl` in the DB, and retries once. Immediately after, `apply_metadata()` runs (see stage 7 dispatch); on success `isMetadataFilled` is flipped, on failure the row is left for stage 7 to retry.
7. **Metadata fill / format dispatch.** `fill_metadata_for_existing()` scans `isDownloaded = 1 AND isMetadataFilled = 0` (covers stage 6 failures plus legacy DB rows). Prefers the stored `filePath` if it still exists, otherwise probes `<sanitizedName>.{flac,wav,mp3}`. `src/metadata.rs` dispatches by extension:
   - `.wav` → spawn system `ffmpeg` to transcode to FLAC, delete the original `.wav`, then fall through to FLAC tagging. The returned path (`Song.filePath` is updated to the new `.flac`).
   - `.flac` → `metaflac::Tag` rewrites Vorbis comments in place (TITLE/ARTIST/ALBUM/ALBUMARTIST/TRACKNUMBER/DESCRIPTION (= album intro)/LYRICS from the sibling `.lrc`) and embeds `cover.jpg` as a `CoverFront` picture block. No re-encoding, no temp file.
   - `.mp3` → `id3::Tag` checks if title/artist/album/cover are already present; if any are missing, writes TIT2/TPE1/TALB/TPE2/TRCK/COMM/USLT plus an APIC cover frame (ID3v2.4).
   - other extensions → no-op.

### Persistence (`src/db.rs`)

`rusqlite` (bundled SQLite, no ORM) opens `database.sqlite` at the project root (gitignored) and executes a DDL that is **byte-identical to the legacy TypeORM-generated schema** — an old database from the previous TypeScript implementation continues to work as-is. All queries are hand-written SQL in this module; camelCase column names (`isCoverDownloaded`, `trackNumber`, …) are kept for schema compatibility.

State flags: `isCoverDownloaded`, `isCoverDeDownloaded` on `album`; `isDownloaded`, `isMetadataFilled` on `song`, plus `sourceUrl` / `lyricUrl` (populated lazily in stage 5) and `filePath` (final on-disk path, may flip from `.wav` to `.flac` after stage 7). Deleting `database.sqlite` is the supported "redo from scratch" reset (already-present audio files on disk will be silently overwritten because `download.rs` does not check existence).

### API client (`src/api.rs`)

Wraps four unauthenticated endpoints under `https://monster-siren.hypergryph.com/api`: `/albums`, `/songs`, `/song/:cid`, `/album/:cid/detail`. Responses are typed serde structs unwrapped from the `{ code, msg, data }` envelope. Note that the API returns `cid`/`albumCid` as **strings**; `cid_from_str` (a serde visitor) coerces them to `i64`.

### Download helper (`src/download.rs`)

`download_file()` creates the destination directory if needed, then streams the response body to disk chunk-by-chunk. It checks the HTTP status and retries up to 3 times with linear backoff on failure. No resume, no progress bar — keep this in mind when extending.

## Conventions specific to this repo

- **Album folder names are not sanitized**, only song filenames are (`sanitize_filename` in `main.rs`). If an album name contains characters illegal on the target filesystem, the directory creation in `download.rs` will fail.
- **DB writes are synchronous** (rusqlite called directly inside async fns). Fine for this sequential CLI; move to `spawn_blocking` if downloads are ever parallelized (noted with a `ponytail:` comment in `main.rs`).
- **ffmpeg is the only external binary dependency**, and only for WAV→FLAC. Everything else (tagging, cover embedding) is in-process.
