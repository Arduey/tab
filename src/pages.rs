//! 页面处理器：安装向导、登录、首页、管理台。

use crate::api::{authorize, cookie_of, App, AppState};
use crate::auth;
use crate::db::{self, Db};
use crate::templates::{self, esc, PageCtx};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use std::collections::HashMap;

// ---------------------------------------------------------------- 响应工具

pub fn redirect(to: &str) -> Response {
    (
        StatusCode::SEE_OTHER,
        [(
            header::LOCATION,
            HeaderValue::from_str(to).unwrap_or(HeaderValue::from_static("/")),
        )],
    )
        .into_response()
}

fn redirect_with_session(to: &str, sid: &str) -> Response {
    let cookie = format!("tab_session={sid}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000");
    (
        StatusCode::SEE_OTHER,
        [
            (
                header::LOCATION,
                HeaderValue::from_str(to).unwrap_or(HeaderValue::from_static("/")),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(&cookie).unwrap_or(HeaderValue::from_static("x=1")),
            ),
        ],
    )
        .into_response()
}

fn html(body: String) -> Response {
    let mut resp = Html(body).into_response();
    // 页面都是动态内容，不让代理缓存
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, must-revalidate"),
    );
    resp
}

// ---------------------------------------------------------------- 表单解析

pub fn parse_form(body: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for kv in body.split('&') {
        let mut it = kv.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        if k.is_empty() {
            continue;
        }
        map.insert(percent_decode(k), percent_decode(v));
    }
    map
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------- 安装向导

pub async fn install_page(State(st): State<App>) -> Response {
    if db::is_installed(&st.db) {
        return redirect("/");
    }
    html(templates::layout(
        "安装向导",
        &install_form("", &HashMap::new(), true),
        "",
        "",
    ))
}

fn install_form(error: &str, v: &HashMap<String, String>, first: bool) -> String {
    let g = |k: &str, default: &str| -> String {
        v.get(k)
            .map(|s| s.as_str())
            .unwrap_or(default)
            .to_string()
    };
    let _ = first;
    let err_html = if error.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="form-err" role="alert">{}</p>"#, esc(error))
    };
    format!(
        r#"<main class="auth-wrap">
  <section class="auth-card">
    <header class="auth-head">
      <img src="/static/v3/logo.svg" alt="" width="40" height="40">
      <div>
        <h1>安装向导</h1>
        <p class="muted">创建唯一的拥有者账号。本站不开放注册，之后只能从这里登录。</p>
      </div>
    </header>
    <form method="post" action="/install" class="stack" autocomplete="off">
      {err_html}
      <label>站点名称
        <input name="site_title" value="{site_title}" placeholder="我的导航" maxlength="60" required>
      </label>
      <label>用户名
        <input name="username" value="{username}" placeholder="admin" maxlength="64" required autocapitalize="off">
      </label>
      <div class="row2">
        <label>密码
          <input name="password" type="password" placeholder="至少 8 位" minlength="8" required>
        </label>
        <label>确认密码
          <input name="password2" type="password" placeholder="再输入一次" minlength="8" required>
        </label>
      </div>
      <button class="btn primary block" type="submit">完成安装</button>
      <p class="muted small">数据保存在程序的 data 目录（SQLite 单文件 + 图标缓存）。忘记密码时删除 <code>data/tab.sqlite3</code> 重新安装即可。</p>
    </form>
  </section>
</main>"#,
        err_html = err_html,
        site_title = esc(&g("site_title", "")),
        username = esc(&g("username", "")),
    )
}

pub async fn install_submit(State(st): State<App>, body: String) -> Response {
    if db::is_installed(&st.db) {
        return redirect("/");
    }
    let form = parse_form(&body);
    let site_title = form.get("site_title").cloned().unwrap_or_default();
    let username = form.get("username").cloned().unwrap_or_default();
    let password = form.get("password").cloned().unwrap_or_default();
    let password2 = form.get("password2").cloned().unwrap_or_default();

    let mut e = String::new();
    if site_title.trim().is_empty() {
        e = "站点名称不能为空".into();
    } else if site_title.trim().chars().count() > 60 {
        e = "站点名称不超过 60 字".into();
    } else if username.trim().len() < 2 {
        e = "用户名至少 2 个字符".into();
    } else if password.len() < 8 {
        e = "密码至少 8 位".into();
    } else if password != password2 {
        e = "两次输入的密码不一致".into();
    }
    if !e.is_empty() {
        return html(templates::layout(
            "安装向导",
            &install_form(&e, &form, false),
            "",
            "",
        ));
    }

    let hash = match auth::hash_password_async(password.clone()).await {
        Ok(h) => h,
        Err(_) => {
            return html(templates::layout(
                "安装向导",
                &install_form("密码加密失败，请重试", &form, false),
                "",
                "",
            ))
        }
    };
    if db::create_admin(&st.db, username.trim(), &hash).is_err() {
        return html(templates::layout(
            "安装向导",
            &install_form("写入数据库失败", &form, false),
            "",
            "",
        ));
    }
    let _ = db::set_setting(&st.db, "site_title", site_title.trim());
    seed_defaults(&st.db);
    let sid = new_session(&st);
    redirect_with_session("/", &sid)
}

fn seed_defaults(db: &Db) {
    if !db::list_categories(db).is_empty() {
        return;
    }
    if let Ok(conn) = db.get() {
        let now = auth::now();
        let _ = conn.execute(
            "INSERT INTO categories(name,icon,color,sort,builtin,created_at) VALUES('Docker','docker','',0,1,?1)",
            [now],
        );
        let _ = conn.execute(
            "INSERT INTO categories(name,icon,color,sort,builtin,created_at) VALUES('常用','star','',10,0,?1)",
            [now],
        );
    }
}

fn new_session(st: &AppState) -> String {
    let sid = auth::random_hex(32);
    db::create_session(&st.db, &sid, 60 * 60 * 24 * 30);
    sid
}

// ---------------------------------------------------------------- 登录

pub async fn login_page(State(st): State<App>) -> Response {
    if !db::is_installed(&st.db) {
        return redirect("/install");
    }
    html(templates::layout("登录", &login_form("", ""), "", ""))
}

fn login_form(error: &str, username: &str) -> String {
    let err_html = if error.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="form-err" role="alert">{}</p>"#, esc(error))
    };
    format!(
        r#"<main class="auth-wrap">
  <section class="auth-card">
    <header class="auth-head">
      <img src="/static/v3/logo.svg" alt="" width="40" height="40">
      <div>
        <h1>登录</h1>
        <p class="muted">仅拥有者账号可登录，不开放注册。</p>
      </div>
    </header>
    <form method="post" action="/login" class="stack" autocomplete="off">
      {err_html}
      <label>用户名
        <input name="username" value="{username}" required autocapitalize="off" autofocus>
      </label>
      <label>密码
        <input name="password" type="password" required>
      </label>
      <button class="btn primary block" type="submit">登录</button>
      <p class="muted small"><a href="/">← 返回首页</a></p>
    </form>
  </section>
</main>"#,
        err_html = err_html,
        username = esc(username)
    )
}

pub async fn login_submit(State(st): State<App>, body: String) -> Response {
    if !db::is_installed(&st.db) {
        return redirect("/install");
    }
    let form = parse_form(&body);
    let username = form.get("username").cloned().unwrap_or_default();
    let password = form.get("password").cloned().unwrap_or_default();
    let Some((user, hash)) = db::admin_credentials(&st.db) else {
        return redirect("/install");
    };
    // 防时序攻击：无论如何都跑一次校验
    let ok = auth::verify_password_async(password.clone(), hash.clone()).await;
    if !ok || username.trim() != user {
        return html(templates::layout(
            "登录",
            &login_form("用户名或密码不正确", username.trim()),
            "",
            "",
        ));
    }
    let sid = new_session(&st);
    redirect_with_session("/manage", &sid)
}

pub async fn logout(State(st): State<App>, headers: HeaderMap) -> Response {
    if let Some(sid) = cookie_of(&headers, "tab_session") {
        db::destroy_session(&st.db, &sid);
    }
    let clear = "tab_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";
    (
        StatusCode::SEE_OTHER,
        [
            (
                header::LOCATION,
                HeaderValue::from_static("/"),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_static(clear),
            ),
        ],
    )
        .into_response()
}

// ---------------------------------------------------------------- 首页

pub async fn index(State(st): State<App>, headers: HeaderMap) -> Response {
    if !db::is_installed(&st.db) {
        return redirect("/install");
    }
    let auth = authorize(&st, &headers);
    let settings = db::public_settings(&st.db);
    let categories = db::list_categories(&st.db);
    let tags = db::list_tags(&st.db);

    let s = |k: &str, d: &str| -> String { settings.get(k).and_then(|v| v.as_str()).map(|v| v.to_string()).unwrap_or_else(|| d.to_string()) };
    let site_title = s("site_title", "我的导航");
    let theme = s("theme", "auto");
    let accent = s("accent", "#3b82f6");
    let engine = s("search_engine", "https://www.google.com/search?q=%s");
    let open_new = settings
        .get("open_in_new_tab")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    html(templates::index_page(&PageCtx {
        site_title: &site_title,
        categories: &categories,
        tags: &tags,
        logged_in: auth.is_some(),
        theme: &theme,
        accent: &accent,
        open_in_new_tab: open_new,
        search_engine: &engine,
    }))
}

// ---------------------------------------------------------------- 管理台

pub async fn manage(State(st): State<App>, headers: HeaderMap) -> Response {
    if !db::is_installed(&st.db) {
        return redirect("/install");
    }
    if authorize(&st, &headers).is_none() {
        return redirect("/login");
    }
    let settings = db::public_settings(&st.db);
    let categories = db::list_categories(&st.db);
    let tags = db::list_tags(&st.db);
    let tokens = db::list_tokens(&st.db);
    let site_title = settings
        .get("site_title")
        .and_then(|v| v.as_str())
        .unwrap_or("我的导航")
        .to_string();

    let data = serde_json::json!({
        "settings": settings,
        "categories": categories,
        "tags": tags,
        "tokens": tokens,
        "builtin_icons": crate::icons::catalog(),
    });
    let bootstrap = format!(
        r#"<script id="bootstrap" type="application/json">{}</script>"#,
        serde_json::to_string(&data)
            .unwrap_or_else(|_| "{}".into())
            .replace('<', "\\u003c")
    );
    let body = templates::manage_page(&site_title, &bootstrap);
    html(templates::layout(
        &format!("管理 · {}", site_title),
        &body,
        "",
        r#"<script src="/static/v3/manage.js" defer></script>"#,
    ))
}

// ---------------------------------------------------------------- 快速添加（书签栏小工具）

pub async fn quick_page(State(st): State<App>, headers: HeaderMap) -> Response {
    if !db::is_installed(&st.db) {
        return redirect("/install");
    }
    let authed = authorize(&st, &headers).is_some();
    let categories = db::list_categories(&st.db);
    let mut opts = String::new();
    for c in &categories {
        opts.push_str(&format!(
            r#"<option value="{id}">{name}</option>"#,
            id = c.id,
            name = esc(&c.name)
        ));
    }
    let body = format!(
        r#"<main class="auth-wrap">
  <section class="auth-card">
    <header class="auth-head">
      <img src="/static/v3/logo.svg" alt="" width="40" height="40">
      <div><h1>快速添加</h1><p class="muted">也可以直接在地址栏用 /quick?url=...&name=... 调用。</p></div>
    </header>
    {authed_note}
    <form id="quick-form" class="stack">
      <label>链接
        <input name="url" id="q-url" placeholder="https://example.com" required>
      </label>
      <label>名称（留空自动用域名）
        <input name="name" id="q-name" placeholder="可选" maxlength="80">
      </label>
      <label>分类
        <select name="category_id" id="q-cat">{opts}</select>
      </label>
      <button class="btn primary block" type="submit">添加</button>
      <p class="muted small" id="q-msg"></p>
      <p class="muted small"><a href="/">← 返回首页</a></p>
    </form>
  </section>
</main>
<script>
(function(){{
  const q = new URLSearchParams(location.search);
  if (q.get('url')) document.getElementById('q-url').value = q.get('url');
  if (q.get('name')) document.getElementById('q-name').value = q.get('name');
  document.getElementById('quick-form').addEventListener('submit', async (e) => {{
    e.preventDefault();
    const msg = document.getElementById('q-msg');
    const payload = {{
      url: document.getElementById('q-url').value,
      name: document.getElementById('q-name').value || new URL(document.getElementById('q-url').value.startsWith('http') ? document.getElementById('q-url').value : 'https://' + document.getElementById('q-url').value).hostname,
      category_id: Number(document.getElementById('q-cat').value),
    }};
    const r = await fetch('/api/tags', {{method:'POST', headers:{{'Content-Type':'application/json'}}, body: JSON.stringify(payload)}});
    const j = await r.json();
    msg.textContent = j.ok ? '已添加 ✓' : ('失败：' + j.error);
  }});
}})();
</script>"#,
        authed_note = if authed {
            ""
        } else {
            r#"<p class="form-err">你还没登录，请先<a href="/login">登录</a>后刷新本页。</p>"#
        },
        opts = opts
    );
    html(templates::layout("快速添加", &body, "", ""))
}