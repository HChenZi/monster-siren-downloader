import {
  Entity,
  Column,
  PrimaryColumn,
  ManyToMany,
  OneToMany,
  CreateDateColumn,
  UpdateDateColumn,
} from "typeorm";

@Entity()
export class Album {
  @PrimaryColumn()
  cid: number;

  @Column()
  name: string;

  @Column()
  coverUrl: string;

  @Column({ default: false })
  isCoverDownloaded: boolean;

  @Column()
  artistes: string;

  @CreateDateColumn()
  createdAt: Date;

  @UpdateDateColumn()
  updatedAt: Date;
}
