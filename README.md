下载塞壬唱片官网的全部歌曲，包括歌曲、封面、歌词信息

## 使用说明

### 1. 安装 Node.js

### 2. 安装依赖

```bash
npm install
```

### 3. 修改配置（可选项）

- 在 `.env` 文件中修改 `SAVE_PATH` 变量，指定保存路径，默认为 `./arknights/` 文件夹

- 由于歌曲名中包含一些 windows 不支持的字符，所以会默认替换为下划线，如果想要替换为其它字符，可以在 `.env` 修改 `REPLACE_CHAR` 变量

### 4. 运行

```bash
npm run start
```
