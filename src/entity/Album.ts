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

  @Column({ nullable: true })
  intro: string;

  @Column({ nullable: true })
  belong: string;

  @Column({ nullable: true })
  coverDeUrl: string;

  @Column({ default: false })
  isCoverDeDownloaded: boolean;

  @CreateDateColumn()
  createdAt: Date;

  @UpdateDateColumn()
  updatedAt: Date;
}
