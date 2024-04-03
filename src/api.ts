import fetch from "node-fetch";

const BASE_URL = "https://monster-siren.hypergryph.com/api";

export const getAlbums = async () => {
  const res: any = await (await fetch(`${BASE_URL}/albums`)).json();
  return res.data;
};

export const getSongs = async () => {
  const res: any = await (await fetch(`${BASE_URL}/songs`)).json();
  return res.data;
};

export const getSongDetail = async (cid: number) => {
  const res: any = await (await fetch(`${BASE_URL}/song/${cid}`)).json();
  return res.data;
};
