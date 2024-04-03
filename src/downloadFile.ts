import fetch from "node-fetch";
import fs from "fs";
import path from "path";
import { pipeline } from "node:stream";
import { promisify } from "node:util";

export default async (url: string, dest: string) => {
  const dir = path.dirname(dest);

  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }

  return fetch(url).then(
    (res) =>
      new Promise((resolve) => {
        const fileStream = fs.createWriteStream(dest);
        resolve(promisify(pipeline)(res.body, fileStream));
      }),
  );
};
