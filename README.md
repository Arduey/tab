# Tab — 个人标签页

一个自托管的个人导航页：分类 + 标签、图标自动抓取并本地缓存、底部悬浮 Docker 栏、
导入导出、带 token 的添加 API。**不开放注册**，唯一的拥有者账号在安装向导中创建。

- Rust（axum + rusqlite），编译成单个可执行文件
- 前端是服务端渲染的 HTML + 少量原生 JS，无框架、无构建链
- 数据只有一个 SQLite 文件加两个资源目录，备份就是复制 `data/`

## 特性

| | |
|---|---|
| 分类 | 新建 / 重命名 / 改色 / 删除；拖拽排序与跨分类移动 |
| 标签 | 名称、链接、描述、图标；拖拽排序；记录点击次数 |
| 图标 | 网站 favicon 自动抓取并缓存到本地、内置 45+ 图标、可上传图片 |
| Docker 栏 | 底部悬浮条（类似 Windows 任务栏），是内置分类，不可删除 |
| 搜索 | 顶部即时过滤；`/` 或 `Ctrl/⌘+K` 聚焦；无匹配时回车直接搜索 |
| 主题 | 跟随系统 / 浅色 / 深色，主色可配 |
| 导入导出 | 完整 JSON 备份与恢复；浏览器书签 HTML 导入 |
| API | 令牌鉴权，`POST /api/tags` 或 `GET /api/quick` 一行加标签 |
| 性能 | 静态资源内嵌 + 强缓存、gzip、SQLite WAL、图标本地化零外链 |

## 宝塔部署（推荐：二进制 + 守护进程）

📖 **完整步骤见 [docs/DEPLOY-BAOTA.md](docs/DEPLOY-BAOTA.md)** —— 含权限设置、Supervisor 配置、
Nginx 反向代理、HTTPS、升级与回滚、排错表。下面是快速版：

1. 把 Linux 静态二进制放到数据目录 `/www/wwwroot/tab-data/`，重命名为 `tab`，
   权限 `755`、属主 `www`。
   | 获取方式 | 说明 |
   |---|---|
   | GitHub Actions | 每次推 `main` 产出 `tab-linux-x86_64` 产物；打 `v*` tag 会自动发 Release |
   | 本地交叉编译 | `rustup target add x86_64-unknown-linux-musl` 后 `cargo build --release --target x86_64-unknown-linux-musl`（需 musl 工具链） |
   | 本机原生编译 | `cargo build --release`，产物 `target/release/tab` |

   产物是 **musl 静态链接**，不依赖 glibc，CentOS 7 到 Debian 12 通用。
2. 宝塔面板 → **网站 → 添加站点**，域名填你的域名，PHP 选「纯静态」。
3. 网站设置 → **反向代理 → 添加反向代理**：
   - 目标 URL：`http://127.0.0.1:8080`
   - 发送域名：`$host`
4. 网站设置 → **配置文件**，在 `location /` 里补上真实 IP 透传（否则日志里全是 127.0.0.1）：

   ```nginx
   proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
   proxy_set_header X-Real-IP $remote_addr;
   proxy_set_header X-Forwarded-Proto $scheme;
   proxy_http_version 1.1;
   proxy_read_timeout 120s;   # 导入大量书签时抓图标会慢，别让它超时
   ```

5. 用宝塔的 **Supervisor 管理器** 守护进程：
   - 名称：`tab`
   - 运行目录：`/www/wwwroot/tab-data`
   - 启动命令：`/www/wwwroot/tab-data/tab`
   - 进程数量：1
   - 环境变量：`TAB_PORT=8080`、`TAB_BIND=127.0.0.1`、
     `TAB_DATA_DIR=/www/wwwroot/tab-data`
6. 浏览器打开域名 → 自动跳到安装向导 → 填站点名、用户名、密码 → 完成。
   安装完成后 `/install` 会永久失效。

反向代理方式由 Nginx 终结 HTTPS，二进制只监听 127.0.0.1:8080，
外网无需放行该端口。升级时替换文件并在 Supervisor 里重启即可。

## Docker 部署

```bash
# 直接跑（镜像自己 build）
docker build -t tab:latest .
docker run -d --name tab --restart unless-stopped \
  -p 8080:8080 -v $(pwd)/data:/data tab:latest

# 或者用 compose
docker compose up -d
```

容器内数据在 `/data`，务必挂卷，否则重建容器会丢数据。

## 环境变量

| 变量 | 默认 | 说明 |
|---|---|---|
| `TAB_PORT` | `8080` | 监听端口 |
| `TAB_BIND` | `0.0.0.0` | 监听地址 |
| `TAB_DATA_DIR` | `./data` | 数据目录（数据库 + 图标缓存 + 上传图标） |

站点名称、主题、主色、搜索地址、账号密码都存在数据库里，在「管理 → 站点设置」中修改，
不需要手动编辑任何文件。忘记密码时删除 `data/tab.sqlite3` 重新安装（会丢数据，
先把 `data/` 整个备份）。

## API

在「管理 → API」创建令牌，令牌只显示一次。

```bash
TOKEN=tab_xxxxxxxx

# 读取全部分类与标签
curl -H "Authorization: Bearer $TOKEN" http://your-host/api/state

# 添加标签（category_id 可省略，会落到第一个普通分类）
curl -X POST http://your-host/api/tags \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"name":"GitHub","url":"https://github.com","category_id":2}'

# 一行添加：适合 iOS 快捷指令 / 书签栏小书签
curl -H "Authorization: Bearer $TOKEN" \
  "http://your-host/api/quick?url=https://example.com&name=Example"

# 导出 / 导入
curl -H "Authorization: Bearer $TOKEN" http://your-host/api/export -o backup.json
curl -X POST http://your-host/api/import \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  --data-binary @backup.json
```

完整接口：

| 方法 | 路径 | 说明 |
|---|---|---|
| `GET` | `/api/state` | 分类、标签、设置、内置图标 |
| `POST` | `/api/tags` | 新增标签（`fetch_icon:false` 可跳过抓取） |
| `PUT` `DELETE` | `/api/tags/:id` | 修改 / 删除标签 |
| `POST` | `/api/tags/reorder` | 批量写下 `{id,category_id,sort}` |
| `POST` | `/api/tags/:id/refetch-icon` | 重新抓取图标 |
| `POST` `PUT` | `/api/categories` | 新建 / 批量排序 |
| `PUT` `DELETE` | `/api/categories/:id` | 修改 / 删除（内置分类拒绝删除） |
| `GET` | `/api/quick` | 查询参数一行加标签，已存在则返回原 id |
| `GET` | `/api/export` | 导出 JSON |
| `POST` | `/api/import` | 导入 JSON，`mode` 为 `merge` 或 `replace` |
| `POST` | `/api/import/bookmarks` | 导入浏览器书签 HTML（body 为文件原文） |
| `GET` `POST` | `/api/tokens` | 列出 / 创建令牌（仅登录会话） |
| `PUT` | `/api/settings` | 站点设置（仅登录会话） |
| `PUT` | `/api/account` | 改用户名 / 密码（需当前密码） |

## 目录结构

```
src/
  main.rs        启动、优雅退出、--health 探针
  api.rs         路由、鉴权、全部 API
  pages.rs       安装向导、登录、首页、管理台、快速添加页
  templates.rs   服务端 HTML 渲染
  db.rs          SQLite schema 与全部查询
  favicon.rs     favicon 抓取与本地缓存
  icons.rs       内置图标库（内联 SVG）
  assets.rs      静态资源内嵌
  auth.rs        密码哈希、会话、时间
  config.rs      环境变量配置
web/static/      纯静态的 CSS / JS
```

## 安全说明

- 只有安装时创建的一个账号，没有注册入口；`/install` 在安装完成后失效
- 密码用 Argon2 加盐哈希；会话是随机 32 字节 ID，存服务端并在 30 天后过期
- API 令牌只存 SHA-256 哈希，权限分 `read` / `write`
- 全部 SQL 使用参数绑定；图标上传会校验魔术字节并只允许 `u-<hex>.<ext>` 文件名
- 所有用户输入在输出前做 HTML 转义