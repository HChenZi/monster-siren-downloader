import "reflect-metadata";
import { DataSource } from "typeorm";
import { Album } from "./entity/Album.js";
import { Song } from "./entity/Song.js";

export const AppDataSource = new DataSource({
  type: "sqlite",
  database: "database.sqlite",
  synchronize: true,
  logging: false,
  entities: [Album, Song],
  migrations: [],
  subscribers: [],
});
