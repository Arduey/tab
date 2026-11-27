//! HTML 模板：服务端渲染整页，前端只做局部交互（无构建链、无框架）。
//! 首次访问是一次请求直接出内容，避免白屏和二次请求。

use crate::db::{Category, Tag};

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn esc_attr(s: &str) -> String {
    esc(s)
}

pub struct PageCtx<'a> {
    pub site_title: &'a str,
    pub categories: &'a [Category],
    pub tags: &'a [Tag],
    pub logged_in: bool,
    pub theme: &'a str,
    pub accent: &'a str,
    pub open_in_new_tab: bool,
    pub search_engine: &'a str,
}

pub fn layout(title: &str, body: &str, extra_head: &str, extra_js: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<meta name="color-scheme" content="light dark">
<title>{title}</title>
<link rel="stylesheet" href="/static/v3/app.css">
<link rel="icon" type="image/svg+xml" href="/static/v3/logo.svg">
{extra_head}
<script src="/static/v3/app.js" defer></script>
{extra_js}
</head>
<body>
{body}
</body>
</html>"#,
        title = esc(title),
        body = body,
        extra_head = extra_head,
        extra_js = extra_js
    )
}

/// 首页：分类区块 + 标签卡片 + 底部悬浮 docker 栏
pub fn index_page(ctx: &PageCtx<'_>) -> String {
    let mut sections = String::new();
    for cat in ctx.categories {
        let items: Vec<&Tag> = ctx
            .tags
            .iter()
            .filter(|t| t.category_id == cat.id)
            .collect();
        let count = items.len();
        let body = if items.is_empty() {
            r#"<p class="empty">这个分类还是空的，点右上角「管理」添加。</p>"#.to_string()
        } else {
            let mut s = String::from(r#"<div class="grid" data-dropzone="1">"#);
            for t in &items {
                s.push_str(&tag_card(t, ctx.open_in_new_tab));
            }
            s.push_str("</div>");
            s
        };
        sections.push_str(&format!(
            r#"<section class="cat" data-cat="{id}" data-cat-name="{name_attr}">
  <header class="cat-head">
    <h2><span class="cat-dot" style="background:{color}"></span>{name}</h2>
    <span class="cat-count">{count}</span>
  </header>
  {body}
</section>"#,
            id = cat.id,
            name = esc(&cat.name),
            name_attr = esc_attr(&cat.name),
            color = if cat.color.is_empty() {
                "var(--accent)".to_string()
            } else {
                esc(&cat.color)
            },
            count = count,
            body = body
        ));
    }

    let docker_tags: Vec<&Tag> = ctx
        .categories
        .iter()
        .filter(|c| c.builtin)
        .flat_map(|c| ctx.tags.iter().filter(move |t| t.category_id == c.id))
        .collect();
    let mut docker_items = String::new();
    for t in &docker_tags {
        docker_items.push_str(&format!(
            r#"<a class="dock-item" href="{url}" target="{target}" rel="noopener" title="{title}">
  {icon}
  <span class="dock-label">{name}</span>
</a>"#,
            url = esc_attr(&t.url),
            target = if ctx.open_in_new_tab { "_blank" } else { "_self" },
            title = esc(&format!("{} · {}", t.name, t.url)),
            icon = icon_html(t, 22),
            name = esc(&t.name)
        ));
    }
    if docker_items.is_empty() {
        docker_items.push_str(r#"<span class="dock-hint">在「管理」里往 Docker 栏添加服务入口</span>"#);
    }

    let login_link = if ctx.logged_in {
        r#"<a class="icon-btn" href="/manage" title="管理">管理</a>"#
    } else {
        r#"<a class="icon-btn" href="/login" title="登录">登录</a>"#
    };

    let empty_tip = if ctx.categories.is_empty() {
        r#"<p class="empty big">还没有任何分类，去「管理」里创建第一个分类。</p>"#
    } else {
        ""
    };

    let main = format!(
        r#"<div class="app">
  <header class="topbar">
    <div class="brand">
      <img src="/static/v3/logo.svg" alt="" width="26" height="26">
      <h1>{site_title}</h1>
    </div>
    <div class="search-wrap">
      <input id="search" class="search" type="search" placeholder="搜索标签…  ( / 聚焦 )" autocomplete="off" spellcheck="false">
      <div id="search-results" class="search-results" hidden></div>
    </div>
    <nav class="top-actions">
      {login_link}
      <button class="icon-btn" id="theme-btn" title="切换主题（浅色 / 深色 / 跟随系统）">主题</button>
    </nav>
  </header>
  <main class="content">
    {empty_tip}
    {sections}
  </main>
</div>
<div class="dock" id="dock">
  <div class="dock-inner">
    <div class="dock-strip">{docker_items}</div>
  </div>
</div>
<div class="toast" id="toast" hidden></div>"#,
        site_title = esc(ctx.site_title),
        login_link = login_link,
        empty_tip = empty_tip,
        sections = sections,
        docker_items = docker_items
    );

    let prefs = format!(
        r#"<script>window.__TAB__={{theme:{theme},accent:{accent},target:{target},engine:{engine}}};</script>"#,
        theme = serde_json::to_string(ctx.theme).unwrap_or_else(|_| "\"auto\"".into()),
        accent = serde_json::to_string(ctx.accent).unwrap_or_else(|_| "\"#3b82f6\"".into()),
        target = if ctx.open_in_new_tab { "1" } else { "0" },
        engine = serde_json::to_string(ctx.search_engine)
            .unwrap_or_else(|_| "\"https://www.google.com/search?q=%s\"".into()),
    );
    layout(ctx.site_title, &main, &prefs, "")
}

fn tag_card(t: &Tag, new_tab: bool) -> String {
    format!(
        r#"<a class="card" href="{url}" target="{target}" rel="noopener" data-id="{id}" data-name="{name_attr}" data-url="{url_attr}" data-desc="{desc_attr}" draggable="true">
  <span class="card-icon">{icon}</span>
  <span class="card-body">
    <span class="card-name">{name}</span>
    {desc}
  </span>
</a>"#,
        url = esc_attr(&t.url),
        url_attr = esc_attr(&t.url),
        target = if new_tab { "_blank" } else { "_self" },
        id = t.id,
        name = esc(&t.name),
        name_attr = esc_attr(&t.name),
        desc_attr = esc_attr(&t.description),
        icon = icon_html(t, 26),
        desc = if t.description.is_empty() {
            String::new()
        } else {
            format!(r#"<span class="card-desc">{}</span>"#, esc(&t.description))
        }
    )
}

/// 图标来源：自动 favicon（/icons/<id>）、内置图标库（内联 SVG）、上传图（/icons/u-xxx）
pub fn icon_html(t: &Tag, size: u32) -> String {
    let icon = t.icon.trim();
    if icon.is_empty() || icon == "favicon" {
        return format!(
            r#"<img loading="lazy" decoding="async" width="{s}" height="{s}" src="/api/favicon/{id}" alt="" onerror="this.classList.add('icon-fallback');this.replaceWith(window.__tabFallback('{name}'))">"#,
            s = size,
            id = t.id,
            name = esc(&t.name)
        );
    }
    if let Some(key) = icon.strip_prefix("builtin:") {
        return crate::icons::builtin_svg(key, size)
            .unwrap_or_else(|| letter_icon(&t.name, size));
    }
    if let Some(file) = icon.strip_prefix("upload:") {
        return format!(
            r#"<img loading="lazy" decoding="async" width="{s}" height="{s}" src="/icons/{file}" alt="">"#,
            s = size,
            file = esc_attr(file)
        );
    }
    // 直接是 URL 的图标
    format!(
        r#"<img loading="lazy" decoding="async" width="{s}" height="{s}" src="{src}" alt="">"#,
        s = size,
        src = esc_attr(icon)
    )
}

fn letter_icon(name: &str, size: u32) -> String {
    let ch = name.chars().next().unwrap_or('#').to_uppercase().to_string();
    format!(
        r#"<span class="letter-icon" style="width:{s}px;height:{s}px;font-size:{f}px">{c}</span>"#,
        s = size,
        f = (size as f32 * 0.52).round() as u32,
        c = esc(&ch)
    )
}
/// 管理台骨架：数据由 bootstrap JSON 注入，交互全在 manage.js
pub fn manage_page(site_title: &str, bootstrap: &str) -> String {
    format!(
        r##"<div class="app">
  <header class="topbar">
    <div class="brand">
      <img src="/static/v3/logo.svg" alt="" width="26" height="26">
      <h1>管理 · {title}</h1>
    </div>
    <nav class="top-actions">
      <a class="icon-btn" href="/">← 首页</a>
      <button class="icon-btn" id="logout-btn" type="button">退出</button>
    </nav>
  </header>
  <main class="manage">
    <nav class="tabs" role="tablist">
      <button class="tab active" data-tab="tags" type="button">分类与标签</button>
      <button class="tab" data-tab="data" type="button">导入 / 导出</button>
      <button class="tab" data-tab="api" type="button">API</button>
      <button class="tab" data-tab="site" type="button">站点设置</button>
    </nav>

    <section class="panel active" id="panel-tags">
      <div class="panel-head">
        <h2>分类与标签</h2>
        <div class="btn-row">
          <button class="btn" id="add-cat" type="button">+ 新建分类</button>
          <button class="btn primary" id="add-tag" type="button">+ 新建标签</button>
        </div>
      </div>
      <p class="muted small">拖拽卡片可以调整顺序，拖到别的分类里可以换分类。Docker 栏是内置分类，不可删除。</p>
      <div id="cats"></div>
    </section>

    <section class="panel" id="panel-data">
      <div class="panel-head"><h2>导入 / 导出</h2></div>
      <div class="card-block">
        <h3>导出</h3>
        <p class="muted small">导出为 JSON（分类 + 标签，不含密码与 token）。</p>
        <button class="btn" id="export-btn" type="button">下载 JSON</button>
      </div>
      <div class="card-block">
        <h3>导入 JSON</h3>
        <p class="muted small">合并模式会跳过已存在的链接；覆盖模式会先清空现有标签。</p>
        <input type="file" id="import-file" accept="application/json,.json">
        <label class="inline"><input type="checkbox" id="import-replace"> 覆盖现有数据</label>
        <button class="btn" id="import-btn" type="button">开始导入</button>
        <p class="muted small" id="import-msg"></p>
      </div>
      <div class="card-block">
        <h3>导入浏览器书签</h3>
        <p class="muted small">支持 Chrome / Edge / Firefox 导出的 HTML 书签，按文件夹生成分类。</p>
        <input type="file" id="bookmark-file" accept="text/html,.html,.htm">
        <button class="btn" id="bookmark-btn" type="button">开始导入</button>
        <p class="muted small" id="bookmark-msg"></p>
      </div>
    </section>

    <section class="panel" id="panel-api">
      <div class="panel-head"><h2>API</h2></div>
      <div class="card-block">
        <h3>创建访问令牌</h3>
        <p class="muted small">用于脚本或快捷指令快速添加标签。令牌只显示一次，请立刻复制。</p>
        <div class="row3">
          <input id="token-name" placeholder="令牌名称，例如 ios-快捷指令" maxlength="60">
          <select id="token-scope">
            <option value="read,write">读写（可添加/修改标签）</option>
            <option value="read">只读</option>
            <option value="write">只写</option>
          </select>
          <button class="btn primary" id="token-create" type="button">创建</button>
        </div>
        <p class="token-out" id="token-out"></p>
        <ul class="token-list" id="token-list"></ul>
      </div>
      <div class="card-block">
        <h3>接口说明</h3>
        <pre class="code">POST /api/tags
Authorization: Bearer &lt;token&gt;
Content-Type: application/json

{{"name":"GitHub","url":"https://github.com","category_id":2}}

GET  /api/state            读取全部分类与标签
POST /api/quick?url=...&name=...&category_id=...   一行添加
GET  /api/export           导出 JSON
POST /api/import           导入 JSON</pre>
      </div>
    </section>

    <section class="panel" id="panel-site">
      <div class="panel-head"><h2>站点设置</h2></div>
      <form class="card-block stack" id="site-form">
        <label>站点名称
          <input name="site_title" maxlength="60" required>
        </label>
        <label>主题
          <select name="theme">
            <option value="auto">跟随系统</option>
            <option value="light">浅色</option>
            <option value="dark">深色</option>
          </select>
        </label>
        <label>主色
          <input name="accent" type="color" value="#3b82f6">
        </label>
        <label>搜索地址（%s 为关键词占位符）
          <input name="search_engine" maxlength="400">
        </label>
        <label class="inline"><input type="checkbox" name="open_in_new_tab"> 新标签页打开</label>
        <button class="btn primary" type="submit">保存设置</button>
        <p class="muted small" id="site-msg"></p>
      </form>
      <form class="card-block stack" id="account-form">
        <h3>账号</h3>
        <label>用户名
          <input name="username" maxlength="64" required>
        </label>
        <label>当前密码
          <input name="current_password" type="password" required>
        </label>
        <label>新密码（留空则不改）
          <input name="new_password" type="password" minlength="8">
        </label>
        <button class="btn primary" type="submit">更新账号</button>
        <p class="muted small" id="account-msg"></p>
      </form>
    </section>
  </main>
</div>
<div class="modal-root" id="modal-root" hidden></div>
<div class="toast" id="toast" hidden></div>
{bootstrap}"##,
        title = esc(site_title),
        bootstrap = bootstrap
    )
}
