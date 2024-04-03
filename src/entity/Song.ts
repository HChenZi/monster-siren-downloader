import {
  Entity,
  Column,
  PrimaryColumn,
  CreateDateColumn,
  UpdateDateColumn,
} from "typeorm";

@Entity()
export class Song {
  @PrimaryColumn()
  cid: number;

  @Column()
  name: string;

  @Column()
  albumCid: number;

  @Column({ nullable: true })
  sourceUrl: string;

  @Column({ nullable: true })
  lyricUrl: string;

  @Column({ default: false })
  isDownloaded: boolean;

  @Column()
  artistes: string;

  @CreateDateColumn()
  createdAt: Date;

  @UpdateDateColumn()
  updatedAt: Date;
}
