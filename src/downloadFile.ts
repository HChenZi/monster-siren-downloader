import fetch from "node-fetch";
import fs from "fs";
import path from "path";
import { pipeline } from "node:stream";
import { promisify } from "node:util";

const MAX_ATTEMPTS = 3;
const RETRY_DELAY_MS = 1000;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const streamToFile = (body: any, dest: string) =>
  new Promise<void>((resolve, reject) => {
    const fileStream = fs.createWriteStream(dest);
    promisify(pipeline)(body, fileStream).then(
      () => resolve(),
      (error) => {
        fileStream.destroy();
        reject(error);
      },
    );
  });

export default async (url: string, dest: string) => {
  const dir = path.dirname(dest);

  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  let lastError: unknown;
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
    try {
      const res = await fetch(url);
      if (!res.ok) {
        throw new Error(`下载失败 HTTP ${res.status} ${res.statusText}: ${url}`);
      }
      await streamToFile(res.body, dest);
      return;
    } catch (error) {
      lastError = error;
      if (attempt < MAX_ATTEMPTS) {
        await sleep(RETRY_DELAY_MS * attempt);
      }
    }
  }
  throw lastError;
};
