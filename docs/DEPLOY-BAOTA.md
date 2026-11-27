# 宝塔部署完整教程

面向 **宝塔面板 + Nginx 反向代理 + Supervisor 守护** 的方案。
你的服务器上已经放好了 `tab-linux-x86_64`（静态链接，不依赖 glibc，CentOS 7 / Debian / Ubuntu 通用）。

全程只需要在宝塔网页界面操作，不用敲命令行 —— 需要敲的地方我会标出来（就那么两三行）。

---

## 0. 前置检查

去宝塔面板 **软件商店** 确认已安装：

| 软件 | 用途 | 必需 |
|---|---|---|
| Nginx | 反向代理 + HTTPS | ✅ |
| Supervisor 管理器 | 守护进程、开机自启、崩溃重启 | ✅ 强烈建议 |
| 防火墙 / 安全组 | 放行 80 / 443 | ✅ |

> 没有 Supervisor 也可以，但进程挂了不会自动拉起，不推荐。

---

## 1. 决定数据目录

数据全部在一个目录里：

```
/www/wwwroot/tab-data/
├── tab.sqlite3      # 数据库：账号、分类、标签、设置、token
├── icons/           # 自动抓取并缓存的 favicon
└── uploads/         # 你手动上传的图标
```

**备份就是打包这个目录**，换机器只拷它就能完整恢复。

先建这个目录：

面板 → **文件** → 进入 `/www/wwwroot/` → 新建目录 `tab-data` → 右键 → **权限** → 属主设为 `www`，权限 `755`。

---

## 2. 放好程序

面板 → **文件** → 找到 FTP 账号的根目录（就是你上传 `tab-linux-x86_64` 的位置）
→ 把 `tab-linux-x86_64` **移动**到 `/www/wwwroot/tab-data/` 并重命名为 `tab`。

然后设置可执行权限：

面板 → 文件 → 右键 `tab` → **权限** →

- 权限：`755`
- 属主 / 属组：`www` / `www`
- 勾选 **应用到子目录**

> 如果界面里有"设置权限"没生效，可用面板自带的终端跑一行：
> ```bash
> chmod 755 /www/wwwroot/tab-data/tab
> ```

**为什么必须 755**：没有执行位，Supervisor 启动会直接报 `Permission denied`。

---

## 3. 先手动试跑一次

在配置守护进程之前，先确认二进制能在这台机器上跑起来。
面板 → **终端**（或 SSH）：

```bash
cd /www/wwwroot/tab-data
./tab --version
```

正常输出：

```
tab 0.1.0
```

如果这里就报错，说明架构不对（比如服务器是 ARM）。在面板终端执行 `uname -m`：

- `x86_64` → 用现在这个文件，没问题
- `aarch64` / `arm64` → 需要 ARM 版本，告诉我我重新编译

接着正式试跑（`Ctrl+C` 退出）：

```bash
cd /www/wwwroot/tab-data
TAB_PORT=8080 TAB_BIND=127.0.0.1 ./tab
```

看到这样一行就是成功了：

```
INFO tab: listening on http://127.0.0.1:8080 (data dir: /www/wwwroot/tab-data)
```

几点说明：

- `TAB_BIND=127.0.0.1` —— **只监听本机**，外网只能通过 Nginx 访问。
  8080 端口**不需要**在防火墙放行，这样最安全。
- 数据目录默认是当前工作目录，所以先 `cd` 进去再启动。
- 这时数据库已经自动建好了，`ls` 能看到 `tab.sqlite3`。

看到日志后按 `Ctrl+C` 停掉，下一步交给 Supervisor 管。

---

## 4. 配置 Supervisor 守护进程

面板 → **软件商店** → **Supervisor 管理器** → **添加守护进程**。

### 表单填写

| 字段 | 填什么 |
|---|---|
| 名称 | `tab` |
| 启动用户 | `www` |
| 运行目录 | `/www/wwwroot/tab-data` |
| 启动命令 | `/www/wwwroot/tab-data/tab` |
| 进程数量 | `1` |

> **进程数量必须是 1**。程序内部用 SQLite 单文件 + WAL，
> 起多个实例虽然也能跑，但 favicon 抓取会有重复写，没必要。

### 环境变量

Supervisor 管理器底部通常有"环境变量"输入框（如果没有，见下方备选方案）：

```
TAB_PORT=8080
TAB_BIND=127.0.0.1
TAB_DATA_DIR=/www/wwwroot/tab-data
TZ=Asia/Shanghai
```

### 备选：没有环境变量输入框时

改用启动命令内联环境变量（大部分 Supervisor 版本支持）：

```bash
cd /www/wwwroot/tab-data && TAB_PORT=8080 TAB_BIND=127.0.0.1 TAB_DATA_DIR=/www/wwwroot/tab-data TZ=Asia/Shanghai ./tab
```

### 添加后检查

状态应显示 **RUNNING**。点「日志」应看到 `listening on http://127.0.0.1:8080`。

再验证本机可访问（面板终端）：

```bash
curl -I http://127.0.0.1:8080/
```

返回 `303` 或 `200` 都正常（`303` 是跳去安装向导）。

**如果状态是 FATAL 或反复重启**，看日志排查：

| 日志里的报错 | 原因 |
|---|---|
| `Permission denied` | `tab` 文件没有执行位，回到第 2 步设 755 |
| `cannot execute binary file` | 架构不对，`uname -m` 确认后找我要对应版本 |
| `Address already in use` | 8080 被占用，换端口（同时改 Nginx 里的转发地址） |
| `Read-only file system` | 运行目录权限不对，属主改成 `www` |

---

## 5. 配置网站与反向代理

### 5.1 添加站点

面板 → **网站** → **添加站点**：

- 域名：填你的域名（例如 `nav.example.com`）
- 根目录：随意（默认即可，**不会被用到**，因为全部请求都转发给程序）
- PHP 版本：**纯静态**
- 数据库：不创建

### 5.2 添加反向代理

站点 → **设置** → **反向代理** → **添加反向代理**：

| 字段 | 填什么 |
|---|---|
| 代理名称 | `tab` |
| 目标 URL | `http://127.0.0.1:8080` |
| 发送域名 | `$host` |

点提交。

### 5.3 修正 Nginx 配置（重要）

反向代理加好后，点 **配置文件**，确认 `location /` 块里包含这几行（宝塔通常会自动加前两行，后两行常缺）：

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "";
    proxy_read_timeout 120s;
}
```

为什么要这些：

- `X-Forwarded-For` / `X-Real-IP` —— 否则服务端看到的来源全是 `127.0.0.1`
- `X-Forwarded-Proto` —— 让程序知道外面是 HTTPS
- `proxy_read_timeout 120s` —— **导入大量书签时**，逐条抓 favicon 会让请求变长，默认 60s 可能超时
- `Upgrade` / `Connection` —— 保留长连接能力

保存后 Nginx 会自动重载。

---

## 6. 申请 HTTPS

站点 → **SSL** → **Let's Encrypt** → 勾选域名 → 申请。

成功后打开 **强制 HTTPS**。

> 如果申请失败，通常是域名没解析到这台服务器，或 80 端口没放行。
> 先确认：`ping 你的域名` 应该指向服务器 IP。

---

## 7. 完成安装向导

浏览器打开 `https://你的域名`，会自动跳到 `/install`：

1. **站点名称** —— 显示在首页顶部，例如「我的导航」
2. **用户名** —— 你的账号名
3. **密码** —— 至少 8 位，**记牢，没有找回功能**

点「完成安装」。

装完会发生三件事：

- 自动登录并跳到首页
- 自动创建 **Docker**（内置分类，不可删除）和 **常用** 两个分类
- `/install` **永久失效**，再访问只会跳回首页 —— 这就是"不开放注册"的实现方式

> **忘记密码怎么办**：没有找回入口（设计如此）。
> 删掉 `/www/wwwroot/tab-data/tab.sqlite3` 重新安装即可 ——
> 但标签数据会一起没。所以先备份，或者让我给你加一个重置密码的小工具。

---

## 8. 开始使用

### 8.1 添加标签

右上角 **管理** → **+ 新建标签**：

- 名称、链接（不写 `https://` 会自动补）
- 分类
- 图标：默认自动抓网站 favicon；也可选内置图标或上传图片
- 保存后图标在后台异步抓取，稍等几秒刷新即可

### 8.2 调整顺序

管理页里 **直接拖拽**卡片：

- 上下拖 = 同分类内排序
- 拖到别的分类框里 = 换分类

松手即保存，不用点保存按钮。

### 8.3 Docker 栏

首页底部的悬浮条就是它（类似 Windows 任务栏）。

往「管理 → Docker 分类」里加标签，就会出现在栏里。

它不能删除、不能改名 —— 这是有意的，避免误删。

### 8.4 导入浏览器书签

管理 → **导入 / 导出** → **导入浏览器书签**：

1. 浏览器导出书签为 HTML
   - Chrome / Edge：`chrome://bookmarks` → 右上角 ⋮ → 导出书签
   - Firefox：`Ctrl+Shift+O` → 导入和备份 → 导出为 HTML
2. 选择文件 → 开始导入

按书签文件夹自动生成分类，已存在的链接会自动跳过。

> 导入后程序会在后台补齐 favicon，书签多的话要等一会。

### 8.5 快速添加 API

管理 → **API** → 起个名字 → **创建**。
令牌只显示一次（已自动复制到剪贴板），存好。

命令行添加：

```bash
TOKEN=tab_你复制的令牌

# 方式一：JSON
curl -X POST https://你的域名/api/tags \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"GitHub","url":"https://github.com","category_id":2}'

# 方式二：一行搞定，分类可省略
curl -H "Authorization: Bearer $TOKEN" \
  "https://你的域名/api/quick?url=https://example.com&name=Example"
```

手机快捷指令（iOS「快捷指令」App）：

- 添加「获取 URL 内容」动作
- URL 填：`https://你的域名/api/quick?url=[剪贴板]`
- 请求头加 `Authorization: Bearer tab_你的令牌`

以后复制任意链接，运行这个指令就自动进导航页了。

**浏览器小书签**（点一下就把当前页存进去）：
新建一个书签，地址栏粘贴：

```javascript
javascript:fetch('https://你的域名/api/quick?url='+encodeURIComponent(location.href)+'&name='+encodeURIComponent(document.title),{headers:{'Authorization':'Bearer tab_你的令牌'}}).then(()=>alert('已添加'))
```

---

## 9. 升级流程

以后我更新了程序，你按这个顺序操作，**数据不会丢**：

### 9.1 先备份（必做）

面板 → 文件 → 把 `/www/wwwroot/tab-data/` **整个目录压缩**成 zip，下载到本地。

> 程序升级不会动数据库结构之外的东西，但备份是唯一的安全网。

### 9.2 停服务

面板 → Supervisor 管理器 → 找到 `tab` → **停止**。

### 9.3 换文件

把新的 `tab` 文件传到 `/www/wwwroot/tab-data/`，覆盖旧的。
（用 FTP 上传 `tab-linux-x86_64` 再在面板里改名覆盖，或直接传同名文件）

重新设权限：右键 → 权限 → `755`，属主 `www`。

### 9.4 启动并验证

Supervisor → **启动** → 看日志有 `listening on ...` → 浏览器刷新首页。

如果启动失败，看第 4 步的排错表，或把日志发我。

### 9.5 回滚

万一新版有问题：停服务 → 把备份的旧 `tab` 文件放回去 → 启动。
如果数据库也出了问题，解压第 9.1 步的备份覆盖 `tab-data/`。

---

## 10. 日常维护

### 备份

数据目录只有几十 MB（图标缓存占大头），建议每周压一次。

宝塔 → **计划任务** → 添加：

- 任务类型：备份网站 或 Shell 脚本
- 执行周期：每周一凌晨
- 脚本内容：

```bash
tar -czf /www/backup/tab-$(date +%F).tar.gz -C /www/wwwroot tab-data
# 只留最近 8 份
ls -t /www/backup/tab-*.tar.gz | tail -n +9 | xargs -r rm -f
```

### 看日志

Supervisor 管理器 → `tab` → **日志**。
日志按天不切割，跑久了会变大；在意的话在 Supervisor 配置里调，或定期清空。

### 资源占用

- 内存：约 15–30MB（原图缓存都在磁盘，不占内存）
- CPU：空闲时接近 0，只在抓 favicon 和登录校验时短暂波动
- 磁盘：程序 7.4MB + 数据目录

---

## 11. 常见问题

**Q：首页能打开，但图标全是字母？**
favicon 抓取失败。可能原因：服务器访问不了目标网站（内网标签如 `192.168.x.x` 属正常），
或目标网站有反爬。可在管理页点标签的 **⟳** 重新抓取。
抓不到的会自动显示首字母图标，不影响使用。

**Q：能同时开多个实例吗？**
不建议。SQLite 单文件下多实例能跑但没必要，Supervisor 里进程数保持 1。

**Q：怎么换端口？**
改 Supervisor 环境变量 `TAB_PORT`，同时改 Nginx 反向代理里的目标 URL，两边必须一致。

**Q：能挂到子路径吗（比如 `example.com/nav`）？**
当前版本按根路径设计，挂在子路径下静态资源会 404。建议用子域名
（`nav.example.com`），这是最省事也最稳的方式。

**Q：数据能迁到另一台服务器吗？**
能。停服务 → 拷走整个 `tab-data/` 目录 → 在新机器放好 `tab` 二进制 →
拷回数据目录 → 按第 4、5 步配 Supervisor 和 Nginx。账号密码都在库里，直接能用。

**Q：怎么改站点名 / 主题色 / 搜索地址？**
管理 → **站点设置**，改完保存即时生效，不用重启。

**Q：怎么改密码？**
管理 → 站点设置 → **账号** → 输入当前密码 + 新密码。
改完会踢掉其他设备的登录状态。

---

## 附：Nginx 完整配置参考

如果反向代理怎么都配不对，可以直接把站点的 `server` 块替换成下面这份
（把 `nav.example.com` 换成你的域名，SSL 证书路径用宝塔生成的）：

```nginx
server {
    listen 80;
    listen 443 ssl http2;
    server_name nav.example.com;

    # 宝塔申请 SSL 后会自动写入这两行
    ssl_certificate     /www/server/panel/vhost/cert/nav.example.com/fullchain.pem;
    ssl_certificate_key /www/server/panel/vhost/cert/nav.example.com/privkey.pem;

    # 强制 HTTPS（HTTP 请求跳过去）
    error_page 497 https://$host$request_uri;
    if ($server_port !~ 443) {
        rewrite ^(/.*)$ https://$host$1 permanent;
    }

    # 图标和静态资源可以给长缓存，程序内部已带 ETag
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host              $host;
        proxy_set_header X-Real-IP         $remote_addr;
        proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_http_version 1.1;
        proxy_set_header Upgrade    $http_upgrade;
        proxy_set_header Connection "";
        proxy_read_timeout 120s;
        proxy_send_timeout 120s;
    }

    access_log /www/wwwlogs/nav.example.com.log;
    error_log  /www/wwwlogs/nav.example.com.error.log;
}
```

保存后 `nginx -t` 检查语法，通过再重载。
