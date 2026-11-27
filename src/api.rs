//! HTTP 路由与 API。

use crate::auth;
use crate::config::Config;
use crate::db::{self, Db};
use crate::favicon;
use crate::icons;
use crate::pages;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

pub struct AppState {
    pub db: Db,
    pub cfg: Config,
    pub http: reqwest::Client,
}

pub type App = Arc<AppState>;

pub fn router(state: App) -> Router {
    Router::new()
        .route("/install", get(pages::install_page).post(pages::install_submit))
        .route("/login", get(pages::login_page).post(pages::login_submit))
        .route("/logout", post(pages::logout))
        .route("/", get(pages::index))
        .route("/manage", get(pages::manage))
        // 数据 API
        .route("/api/state", get(api_state))
        .route("/api/categories", post(cat_create).put(cat_reorder))
        .route("/api/categories/:id", put(cat_update).delete(cat_delete))
        .route("/api/tags", post(tag_create))
        .route("/api/tags/reorder", post(tag_reorder))
        .route("/api/tags/:id", put(tag_update).delete(tag_delete))
        .route("/api/tags/:id/hit", post(tag_hit))
        .route("/api/tags/:id/refetch-icon", post(tag_refetch_icon))
        // 图标
        .route("/api/favicon/:id", get(serve_favicon))
        .route("/api/builtin-icon/:name", get(builtin_icon_svg))
        .route("/api/builtin-icons.js", get(builtin_icon_bundle))
        .route("/api/icon/upload", post(icon_upload))
        .route("/api/icons", get(icon_list))
        .route("/icons/:name", get(serve_upload))
        // 导入导出
        .route("/api/export", get(export))
        .route("/api/import", post(importer))
        .route("/api/import/bookmarks", post(import_bookmarks))
        // 快捷添加（GET，方便快捷指令 / 书签栏）
        .route("/api/quick", get(quick_add).post(quick_add))
        // 设置
        .route("/api/settings", put(settings_update))
        .route("/api/tokens", get(token_list).post(token_create))
        .route("/api/tokens/:id", delete(token_delete))
        .route("/api/account", put(account_update))
        // 静态资源
        .route("/static/:file", get(static_file))
        .route("/static/:v/:file", get(static_versioned))
        // 快速添加入口（token）
        .route("/quick", get(pages::quick_page))
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}

// ---------------------------------------------------------------- 鉴权

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Session,
    Token,
}

#[derive(Clone)]
pub struct Auth {
    pub who: Who,
    pub scope: &'static str,
    pub session_id: Option<String>,
}

impl Auth {
    pub fn is_session(&self) -> bool {
        self.who == Who::Session
    }
    pub fn can(&self, need: &str) -> bool {
        if self.is_session() {
            return true;
        }
        self.scope.split(',').any(|s| s.trim() == need)
    }
}

pub fn cookie_of(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let p = part.trim();
        if let Some(rest) = p.strip_prefix(name) {
            if let Some(v) = rest.strip_prefix('=') {
                return Some(v.to_string());
            }
        }
    }
    None
}

pub fn authorize(st: &AppState, headers: &HeaderMap) -> Option<Auth> {
    if let Some(bearer) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        let bearer = bearer.trim();
        if bearer.len() < 8 {
            return None;
        }
        return db::check_token(&st.db, bearer).map(|scope| {
            // scope 字符串需要在 'static 生命周期上；这里安全泄漏一次（数量极小、进程级）
            let leaked: &'static str = Box::leak(scope.into_boxed_str());
            Auth {
                who: Who::Token,
                scope: leaked,
                session_id: None,
            }
        });
    }
    if let Some(sid) = cookie_of(headers, "tab_session") {
        if db::session_valid(&st.db, &sid) {
            return Some(Auth {
                who: Who::Session,
                scope: "read,write",
                session_id: Some(sid),
            });
        }
    }
    None
}

/// 需要登录的 API
macro_rules! need_auth {
    ($st:expr, $h:expr) => {
        match authorize(&$st, &$h) {
            Some(a) => a,
            None => return err(StatusCode::UNAUTHORIZED, "未登录或凭证无效"),
        }
    };
}

fn need_scope(auth: &Auth, scope: &str) -> Option<Response> {
    if auth.can(scope) {
        None
    } else {
        Some(err(StatusCode::FORBIDDEN, "该 token 没有此权限"))
    }
}

// ---------------------------------------------------------------- 响应工具

pub fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "ok": false, "error": msg }))).into_response()
}

pub fn ok(v: Value) -> Response {
    Json(json!({ "ok": true, "data": v })).into_response()
}

fn bad_request(msg: &str) -> Response {
    err(StatusCode::BAD_REQUEST, msg)
}

pub fn valid_url(u: &str) -> bool {
    if u.is_empty() || u.len() > 4096 {
        return false;
    }
    let lower = u.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return false;
    }
    url::Url::parse(u).is_ok()
}

/// 规范化输入：没写协议的补 https://；已带任何协议的原样保留
pub fn normalize_url(u: &str) -> String {
    let t = u.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.contains("://") {
        t.to_string()
    } else {
        format!("https://{t}")
    }
}

// ---------------------------------------------------------------- 页面

async fn static_file(Path(file): Path<String>) -> Response {
    let Some(asset) = crate::assets::get(&file) else {
        return err(StatusCode::NOT_FOUND, "not found");
    };
    let etag = crate::assets::etag(&file);
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static(asset.content_type),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
            (
                header::ETAG,
                HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("v")),
            ),
        ],
        asset.body,
    )
        .into_response()
}

/// /static/<v>/<file>：v 只用于浏览器缓存失效，服务端忽略
async fn static_versioned(Path((_v, file)): Path<(String, String)>) -> Response {
    static_file(Path(file)).await
}

// ---------------------------------------------------------------- 状态

async fn api_state(State(st): State<App>, headers: HeaderMap) -> Response {
    let auth = authorize(&st, &headers);
    let categories = db::list_categories(&st.db);
    let tags = db::list_tags(&st.db);
    let settings = db::public_settings(&st.db);
    ok(json!({
        "authed": auth.is_some(),
        "settings": settings,
        "categories": categories,
        "tags": tags,
        "builtin_icons": icons::catalog(),
    }))
}

// ---------------------------------------------------------------- 分类

#[derive(Deserialize)]
struct CatIn {
    name: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    color: String,
}

async fn cat_create(State(st): State<App>, headers: HeaderMap, Json(b): Json<CatIn>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let name = b.name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return bad_request("分类名不能为空且不超过 40 字");
    }
    match db::create_category(&st.db, name, b.icon.trim(), b.color.trim()) {
        Ok(id) => ok(json!({ "id": id })),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn cat_update(
    State(st): State<App>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(b): Json<CatIn>,
) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let name = b.name.trim();
    if name.is_empty() {
        return bad_request("分类名不能为空");
    }
    match db::update_category(&st.db, id, name, b.icon.trim(), b.color.trim()) {
        Ok(_) => ok(json!({ "id": id })),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn cat_delete(State(st): State<App>, headers: HeaderMap, Path(id): Path<i64>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    match db::delete_category(&st.db, id) {
        Ok(true) => ok(json!({ "deleted": id })),
        Ok(false) => bad_request("内置分类不能删除"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct ReorderIn {
    #[serde(default)]
    category_ids: Vec<i64>,
    #[serde(default)]
    tags: Vec<TagPos>,
}

#[derive(Deserialize)]
struct TagPos {
    id: i64,
    category_id: i64,
    sort: i64,
}

async fn cat_reorder(State(st): State<App>, headers: HeaderMap, Json(b): Json<ReorderIn>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    if let Err(e) = db::reorder_categories(&st.db, &b.category_ids) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    ok(json!({ "reordered": b.category_ids.len() }))
}

// ---------------------------------------------------------------- 标签

#[derive(Deserialize)]
struct TagIn {
    category_id: i64,
    name: String,
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon: String,
    /// 是否在保存后自动抓取 favicon
    #[serde(default)]
    fetch_icon: Option<bool>,
}

async fn tag_create(State(st): State<App>, headers: HeaderMap, Json(b): Json<TagIn>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let name = b.name.trim();
    let url = normalize_url(&b.url);
    if name.is_empty() || name.chars().count() > 80 {
        return bad_request("名称不能为空且不超过 80 字");
    }
    if !valid_url(&url) {
        return bad_request("链接必须是 http(s) 地址");
    }
    let cats = db::list_categories(&st.db);
    if !cats.iter().any(|c| c.id == b.category_id) {
        return bad_request("分类不存在");
    }
    let icon = b.icon.trim().to_string();
    let new = db::NewTag {
        category_id: b.category_id,
        name,
        url: &url,
        description: b.description.trim(),
        icon: &icon,
        icon_mime: "",
    };
    let id = match db::create_tag(&st.db, &new) {
        Ok(id) => id,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // 没指定图标就异步抓 favicon（不阻塞响应）
    let want_fetch = b.fetch_icon.unwrap_or(icon.is_empty()) && icon.is_empty();
    if want_fetch {
        let st2 = st.clone();
        let url2 = url.clone();
        tokio::spawn(async move {
            let icons_dir = st2.cfg.icons_dir();
            favicon::fetch_for_tag(&st2.http, &st2.db, id, &url2, &icons_dir).await;
        });
    }
    ok(json!({ "id": id }))
}

async fn tag_update(
    State(st): State<App>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(b): Json<TagIn>,
) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let name = b.name.trim();
    let url = normalize_url(&b.url);
    if name.is_empty() {
        return bad_request("名称不能为空");
    }
    if !valid_url(&url) {
        return bad_request("链接必须是 http(s) 地址");
    }
    let icon = b.icon.trim().to_string();
    match db::update_tag(&st.db, id, b.category_id, name, &url, b.description.trim(), &icon) {
        Ok(_) => ok(json!({ "id": id })),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn tag_delete(State(st): State<App>, headers: HeaderMap, Path(id): Path<i64>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    match db::delete_tag(&st.db, id) {
        Ok(_) => {
            favicon::remove(&st.cfg.icons_dir(), id);
            ok(json!({ "deleted": id }))
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn tag_reorder(State(st): State<App>, headers: HeaderMap, Json(b): Json<ReorderIn>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let items: Vec<(i64, i64, i64)> = b
        .tags
        .iter()
        .map(|t| (t.id, t.category_id, t.sort))
        .collect();
    match db::reorder_tags(&st.db, &items) {
        Ok(_) => ok(json!({ "reordered": items.len() })),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn tag_hit(State(st): State<App>, Path(id): Path<i64>) -> Response {
    db::bump_hit(&st.db, id);
    ok(json!({ "id": id }))
}

async fn tag_refetch_icon(State(st): State<App>, headers: HeaderMap, Path(id): Path<i64>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let tags = db::list_tags(&st.db);
    let Some(tag) = tags.into_iter().find(|t| t.id == id) else {
        return err(StatusCode::NOT_FOUND, "标签不存在");
    };
    let got = favicon::fetch_for_tag(&st.http, &st.db, id, &tag.url, &st.cfg.icons_dir()).await;
    // 让浏览器绕过缓存重新取
    let v = auth::now();
    ok(json!({ "fetched": got.is_some(), "v": v }))
}

async fn serve_favicon(State(st): State<App>, Path(id): Path<i64>) -> Response {
    let path = favicon::icon_path(&st.cfg.icons_dir(), id);
    let Ok(bytes) = tokio::fs::read(&path).await else {
        // 没有缓存时返回一个透明的 1x1，避免 404 噪音（前端会换成字母图标）
        return (
            StatusCode::NO_CONTENT,
            [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        )
            .into_response();
    };
    let mime = db::tag_icon_mime(&st.db, id).unwrap_or_else(|| {
        favicon::sniff(None, &bytes)
            .unwrap_or("image/png")
            .to_string()
    });
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("image/png")),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=2592000"),
            ),
        ],
        bytes,
    )
        .into_response()
}

// ---------------------------------------------------------------- 图标上传

async fn icon_upload(State(st): State<App>, headers: HeaderMap, mut mp: Multipart) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let mut data: Option<Vec<u8>> = None;
    let mut filename = String::from("icon");
    while let Ok(Some(field)) = mp.next_field().await {
        if field.name() == Some("file") {
            filename = field.file_name().unwrap_or("icon").to_string();
            match field.bytes().await {
                Ok(b) => data = Some(b.to_vec()),
                Err(_) => return bad_request("读取上传内容失败"),
            }
        }
    }
    let Some(bytes) = data else {
        return bad_request("没有收到文件");
    };
    if bytes.len() > 2 * 1024 * 1024 {
        return bad_request("图标不能超过 2MB");
    }
    let ext = match favicon::sniff(None, &bytes) {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        Some("image/svg+xml") => "svg",
        Some("image/x-icon") => "ico",
        _ => return bad_request("只支持 png/jpg/gif/webp/svg/ico 图片"),
    };
    let _ = filename;
    let name = format!("u-{}.{}", auth::random_hex(10), ext);
    let dir = st.cfg.uploads_dir();
    if tokio::fs::create_dir_all(&dir).await.is_err() {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "创建目录失败");
    }
    if tokio::fs::write(dir.join(&name), &bytes).await.is_err() {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "写入失败");
    }
    ok(json!({ "icon": format!("upload:{name}"), "url": format!("/icons/{name}") }))
}

async fn serve_upload(State(st): State<App>, Path(name): Path<String>) -> Response {
    // 只允许 u-<hex>.<ext>，防止路径穿越
    if !name.starts_with("u-") || name.contains("..") || name.contains('/') || name.contains('\\') {
        return err(StatusCode::NOT_FOUND, "not found");
    }
    let path = st.cfg.uploads_dir().join(&name);
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return err(StatusCode::NOT_FOUND, "not found");
    };
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    };
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
        ],
        bytes,
    )
        .into_response()
}

async fn icon_list() -> Response {
    ok(json!({ "names": icons::ICON_NAMES }))
}

// ---------------------------------------------------------------- 导入导出

async fn export(State(st): State<App>, headers: HeaderMap) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "read") {
        return r;
    }
    let categories = db::list_categories(&st.db);
    let tags = db::list_tags(&st.db);
    let payload = json!({
        "app": "tab",
        "version": 1,
        "exported_at": auth::now(),
        "categories": categories,
        "tags": tags,
    });
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            ),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=\"tab-backup.json\""),
            ),
        ],
        serde_json::to_string_pretty(&payload).unwrap_or_default(),
    )
        .into_response()
}

#[derive(Deserialize)]
struct ImportIn {
    #[serde(default)]
    mode: String, // merge | replace
    categories: Vec<ImportCat>,
    tags: Vec<ImportTag>,
}

#[derive(Deserialize)]
struct ImportCat {
    #[serde(default)]
    id: i64,
    name: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    color: String,
    #[serde(default)]
    builtin: bool,
}

#[derive(Deserialize)]
struct ImportTag {
    #[serde(default)]
    category_id: i64,
    #[serde(default)]
    category_name: Option<String>,
    name: String,
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon: String,
}

async fn importer(State(st): State<App>, headers: HeaderMap, Json(b): Json<ImportIn>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    match do_import(&st, b).await {
        Ok(v) => ok(v),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn do_import(st: &App, b: ImportIn) -> Result<Value, String> {
    let replace = b.mode == "replace";
    if replace {
        let _ = db::wipe_all(&st.db);
    }

    // 现有分类：name -> id
    let mut existing: HashMap<String, i64> = db::list_categories(&st.db)
        .into_iter()
        .map(|c| (c.name.clone(), c.id))
        .collect();
    let mut old_to_new: HashMap<i64, i64> = HashMap::new();
    let mut created_cats = 0;
    let mut created_tags = 0;
    let mut skipped = 0;

    for c in &b.categories {
        let name = c.name.trim();
        if name.is_empty() {
            continue;
        }
        // 内置分类（Docker 栏）保留原有，不重复建
        if c.builtin {
            if let Some(&id) = existing.get(name) {
                old_to_new.insert(c.id, id);
            }
            continue;
        }
        if let Some(&id) = existing.get(name) {
            old_to_new.insert(c.id, id);
            continue;
        }
        match db::create_category(&st.db, name, c.icon.trim(), c.color.trim()) {
            Ok(id) => {
                old_to_new.insert(c.id, id);
                existing.insert(name.to_string(), id);
                created_cats += 1;
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    let cat_by_name: HashMap<String, i64> = existing.clone();
    let mut pending: Vec<(i64, String, String, String, String)> = Vec::with_capacity(b.tags.len());
    for t in &b.tags {
        let name = t.name.trim();
        let url = normalize_url(&t.url);
        if name.is_empty() || !valid_url(&url) {
            skipped += 1;
            continue;
        }
        let cat_id = old_to_new
            .get(&t.category_id)
            .copied()
            .or_else(|| {
                t.category_name
                    .as_ref()
                    .and_then(|n| cat_by_name.get(n).copied())
            })
            .unwrap_or(0);
        if cat_id == 0 {
            skipped += 1;
            continue;
        }
        pending.push((
            cat_id,
            name.to_string(),
            url,
            t.icon.trim().to_string(),
            t.description.trim().to_string(),
        ));
    }

    // 一次性事务写入，避免每条一个 fsync
    let (c, s) = db::bulk_insert_tags(&st.db, &pending);
    created_tags += c;
    skipped += s;

    // 导入后补抓缺失 favicon（限制并发，后台跑）
    let st2 = st.clone();
    tokio::spawn(async move {
        let tags = db::list_tags(&st2.db);
        let missing: Vec<_> = tags
            .into_iter()
            .filter(|t| t.icon.is_empty())
            .take(120)
            .collect();
        for t in missing {
            let path = favicon::icon_path(&st2.cfg.icons_dir(), t.id);
            if path.exists() {
                continue;
            }
            favicon::fetch_for_tag(&st2.http, &st2.db, t.id, &t.url, &st2.cfg.icons_dir()).await;
        }
    });

    Ok(json!({
        "categories": created_cats,
        "tags": created_tags,
        "skipped": skipped,
        "mode": if replace { "replace" } else { "merge" },
    }))
}

/// 导入浏览器书签 HTML（Chrome / Edge / Firefox 导出的 Netscape 格式）
async fn import_bookmarks(State(st): State<App>, headers: HeaderMap, body: String) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let parsed = match parse_bookmarks(&body) {
        Some(p) => p,
        None => return bad_request("没有解析到任何书签，请确认是浏览器导出的 HTML 文件"),
    };
    let mut created_cats = 0;
    let mut created_tags = 0;
    let mut skipped = 0;
    let mut pending: Vec<(i64, String, String, String, String)> = Vec::new();
    for (folder, links) in parsed {
        let cat_id = match db::find_category_by_name(&st.db, &folder) {
            Some(id) => id,
            None => match db::create_category(&st.db, &folder, "bookmark", "") {
                Ok(id) => {
                    created_cats += 1;
                    id
                }
                Err(_) => continue,
            },
        };
        for (name, url) in links {
            if !valid_url(&url) {
                skipped += 1;
                continue;
            }
            pending.push((cat_id, name, url, String::new(), String::new()));
        }
    }
    let (c, s) = db::bulk_insert_tags(&st.db, &pending);
    created_tags += c;
    skipped += s;
    ok(json!({ "categories": created_cats, "tags": created_tags, "skipped": skipped }))
}

/// 极简 Netscape 书签解析：<DT><H3>文件夹</H3> ... <DT><A HREF="...">名字</A>
/// 一个书签条目：(显示名, 链接)
pub type BookmarkLink = (String, String);
/// 一个书签文件夹：(文件夹名, 里面的书签)
pub type BookmarkFolder = (String, Vec<BookmarkLink>);

pub fn parse_bookmarks(html: &str) -> Option<Vec<BookmarkFolder>> {
    let mut out: Vec<BookmarkFolder> = Vec::new();
    let mut stack: Vec<String> = vec!["未分类".to_string()];
    let mut current: Vec<BookmarkFolder> = vec![("未分类".to_string(), vec![])];
    let mut idx = 0usize;
    let lower = html.to_ascii_lowercase();

    while idx < html.len() {
        let rest = &lower[idx..];
        let next_dt = rest.find("<dt>");
        let next_h3 = rest.find("<h3");
        let next_a = rest.find("<a ");
        let next_dl_end = rest.find("</dl>");
        let pick = [
            next_dt.map(|p| (p, 0)),
            next_h3.map(|p| (p, 1)),
            next_a.map(|p| (p, 2)),
            next_dl_end.map(|p| (p, 3)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(p, _)| *p);

        let Some((pos, kind)) = pick else { break };
        let at = idx + pos;
        match kind {
            1 => {
                // <H3>名字</H3>
                let tail = &html[at..];
                if let Some(gt) = tail.find('>') {
                    let inner_start = at + gt + 1;
                    if let Some(end) = lower[inner_start..].find("</h3>") {
                        let name = strip_tags(&html[inner_start..inner_start + end]);
                        let name = if name.trim().is_empty() {
                            "未分类".to_string()
                        } else {
                            name.trim().to_string()
                        };
                        stack.push(name.clone());
                        current.push((name, vec![]));
                    }
                }
            }
            2 => {
                // <A HREF="url">名字</A>
                let tail_end = lower[at..].find("</a>").map(|p| at + p).unwrap_or(html.len());
                let tag = &html[at..tail_end.min(html.len())];
                let href = attr_of(tag, "href");
                let name = match tag.find('>') {
                    Some(gt) => strip_tags(&tag[gt + 1..]),
                    None => String::new(),
                };
                if let Some(url) = href {
                    // 浏览器导出的 HREF 里 & 会被转义成 &amp;，必须还原，
                    // 否则带查询参数的链接会存错。
                    let url = html_unescape(url.trim());
                    let name = if name.trim().is_empty() {
                        url.clone()
                    } else {
                        name.trim().to_string()
                    };
                    if let Some(last) = current.last_mut() {
                        last.1.push((name, url));
                    }
                }
                idx = tail_end + 4;
                continue;
            }
            3 => {
                // </DL>：结束当前文件夹（最外层保留）
                if current.len() > 1 {
                    if let Some(done) = current.pop() {
                        if !done.1.is_empty() {
                            out.push(done);
                        }
                    }
                    stack.pop();
                }
            }
            _ => {}
        }
        idx = at + 1;
    }
    // 仍在栈里的（嵌套未闭合）
    while let Some(done) = current.pop() {
        if !done.1.is_empty() {
            out.push(done);
        }
    }
    if out.is_empty() {
        return None;
    }
    // 同一文件夹合并
    let mut merged: Vec<BookmarkFolder> = Vec::new();
    for (name, links) in out {
        if let Some(slot) = merged.iter_mut().find(|(n, _)| *n == name) {
            slot.1.extend(links);
        } else {
            merged.push((name, links));
        }
    }
    Some(merged)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    let mut decode_buf = String::new();
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => decode_buf.push(c),
            _ => {}
        }
    }
    out.push_str(&decode_buf);
    html_unescape(out.trim())
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn attr_of(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(name) {
        let i = from + pos;
        let after = &tag[i + name.len()..];
        let trimmed = after.trim_start();
        if let Some(rest) = trimmed.strip_prefix('=') {
            let rest = rest.trim_start();
            let quote = rest.chars().next()?;
            if quote == '"' || quote == '\'' {
                let inner = &rest[1..];
                let close = inner.find(quote)?;
                return Some(inner[..close].to_string());
            }
        }
        from = i + name.len();
    }
    None
}

// ---------------------------------------------------------------- 设置

#[derive(Deserialize)]
struct SettingsIn {
    #[serde(default)]
    site_title: Option<String>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    accent: Option<String>,
    #[serde(default)]
    open_in_new_tab: Option<bool>,
    #[serde(default)]
    search_engine: Option<String>,
    #[serde(default)]
    show_categories: Option<bool>,
}

async fn settings_update(State(st): State<App>, headers: HeaderMap, Json(b): Json<SettingsIn>) -> Response {
    let auth = need_auth!(st, headers);
    if !auth.is_session() {
        return err(StatusCode::FORBIDDEN, "只有登录会话可以改设置");
    }
    if let Some(v) = b.site_title {
        let v = v.trim();
        if v.is_empty() || v.chars().count() > 60 {
            return bad_request("站点名称不能为空且不超过 60 字");
        }
        let _ = db::set_setting(&st.db, "site_title", v);
    }
    if let Some(v) = b.theme {
        let v = match v.as_str() {
            "light" | "dark" | "auto" => v,
            _ => return bad_request("主题只能是 light/dark/auto"),
        };
        let _ = db::set_setting(&st.db, "theme", &v);
    }
    if let Some(v) = b.accent {
        if !(v.is_empty() || (v.starts_with('#') && (v.len() == 7 || v.len() == 4))) {
            return bad_request("主色需要是 #rrggbb 形式");
        }
        let _ = db::set_setting(&st.db, "accent", &v);
    }
    if let Some(v) = b.open_in_new_tab {
        let _ = db::set_setting(&st.db, "open_in_new_tab", if v { "1" } else { "0" });
    }
    if let Some(v) = b.search_engine {
        let v = v.trim();
        if !valid_engine(v) {
            return bad_request("搜索地址必须以 http(s) 开头并包含 %s 占位符");
        }
        let _ = db::set_setting(&st.db, "search_engine", v);
    }
    if let Some(v) = b.show_categories {
        let _ = db::set_setting(&st.db, "show_categories", if v { "1" } else { "0" });
    }
    ok(db::public_settings(&st.db))
}

fn valid_engine(v: &str) -> bool {
    (v.starts_with("http://") || v.starts_with("https://")) && v.contains("%s") && v.len() < 500
}

#[derive(Deserialize)]
struct AccountIn {
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    current_password: String,
    #[serde(default)]
    new_password: Option<String>,
}

async fn account_update(State(st): State<App>, headers: HeaderMap, Json(b): Json<AccountIn>) -> Response {
    let auth = need_auth!(st, headers);
    if !auth.is_session() {
        return err(StatusCode::FORBIDDEN, "只有登录会话可以改账号");
    }
    let Some((user, hash)) = db::admin_credentials(&st.db) else {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "账号数据缺失");
    };
    if !auth::verify_password_async(b.current_password.clone(), hash.clone()).await {
        return bad_request("当前密码不正确");
    }
    let mut new_hash = None;
    if let Some(pw) = b.new_password.as_ref().filter(|p| !p.is_empty()) {
        if pw.len() < 8 {
            return bad_request("新密码至少 8 位");
        }
        match auth::hash_password_async(pw.clone()).await {
            Ok(h) => new_hash = Some(h),
            Err(_) => return err(StatusCode::INTERNAL_SERVER_ERROR, "密码加密失败"),
        }
    }
    let new_user = b
        .username
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != user);
    if let Some(u) = new_user {
        if u.len() < 2 || u.len() > 64 {
            return bad_request("用户名长度 2-64");
        }
    }
    if let Err(e) = db::update_admin(&st.db, new_user, new_hash.as_deref()) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    if new_hash.is_some() {
        // 改密码后踢掉其它会话
        db::destroy_all_sessions(&st.db, auth.session_id.as_deref());
    }
    ok(json!({ "updated": true }))
}

// ---------------------------------------------------------------- token

async fn token_list(State(st): State<App>, headers: HeaderMap) -> Response {
    let auth = need_auth!(st, headers);
    if !auth.is_session() {
        return err(StatusCode::FORBIDDEN, "只有登录会话可以管理 token");
    }
    ok(json!({ "tokens": db::list_tokens(&st.db) }))
}

#[derive(Deserialize)]
struct TokenIn {
    name: String,
    #[serde(default)]
    scope: String,
}

async fn token_create(State(st): State<App>, headers: HeaderMap, Json(b): Json<TokenIn>) -> Response {
    let auth = need_auth!(st, headers);
    if !auth.is_session() {
        return err(StatusCode::FORBIDDEN, "只有登录会话可以创建 token");
    }
    let name = b.name.trim();
    if name.is_empty() {
        return bad_request("给 token 起个名字");
    }
    let scope = if b.scope.trim().is_empty() {
        "read,write".to_string()
    } else {
        let parts: Vec<&str> = b
            .scope
            .split(',')
            .map(|s| s.trim())
            .filter(|s| *s == "read" || *s == "write")
            .collect();
        if parts.is_empty() {
            return bad_request("scope 只能是 read / write");
        }
        parts.join(",")
    };
    match db::create_token(&st.db, name, &scope) {
        Ok(t) => ok(json!({ "token": t, "name": name, "scope": scope })),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn token_delete(State(st): State<App>, headers: HeaderMap, Path(id): Path<i64>) -> Response {
    let auth = need_auth!(st, headers);
    if !auth.is_session() {
        return err(StatusCode::FORBIDDEN, "只有登录会话可以管理 token");
    }
    db::delete_token(&st.db, id);
    ok(json!({ "deleted": id }))
}


// ---------------------------------------------------------------- 内置图标（供前端选择器使用）

async fn builtin_icon_svg(Path(name): Path<String>) -> Response {
    match icons::builtin_svg(&name, 20) {
        Some(svg) => (
            [
                (header::CONTENT_TYPE, HeaderValue::from_static("image/svg+xml")),
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=31536000, immutable"),
                ),
            ],
            svg,
        )
            .into_response(),
        None => err(StatusCode::NOT_FOUND, "unknown icon"),
    }
}

/// 一次性把内置图标 SVG 塞给前端，省掉几十个请求
async fn builtin_icon_bundle() -> Response {
    let mut map = serde_json::Map::new();
    for name in icons::ICON_NAMES {
        if let Some(svg) = icons::builtin_svg(name, 20) {
            map.insert((*name).to_string(), Value::String(svg));
        }
    }
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
        ],
        format!(
            "window.__TAB_ICONS__={};window.__tabBuiltinSvg=function(n,s){{var v=window.__TAB_ICONS__[n];if(!v)return '';return s&&s!==20?v.replace(/(width|height)=\"20\"/g,'$1=\"'+s+'\"'):v;}};",
            serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
        ),
    )
        .into_response()
}

// ---------------------------------------------------------------- 快速添加（GET，便于快捷指令）

#[derive(Deserialize)]
struct QuickIn {
    url: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    category_id: Option<i64>,
    #[serde(default)]
    category: Option<String>,
}

async fn quick_add(State(st): State<App>, headers: HeaderMap, Query(q): Query<QuickIn>) -> Response {
    let auth = need_auth!(st, headers);
    if let Some(r) = need_scope(&auth, "write") {
        return r;
    }
    let url = normalize_url(&q.url);
    if !valid_url(&url) {
        return bad_request("url 参数不是合法的 http(s) 地址");
    }
    if let Some(existing) = db::find_tag_by_url(&st.db, &url) {
        return ok(json!({ "id": existing, "existed": true }));
    }
    // 分类：优先 id，其次名字，最后落到第一个非内置分类
    let cats = db::list_categories(&st.db);
    let cat_id = q
        .category_id
        .filter(|id| cats.iter().any(|c| c.id == *id))
        .or_else(|| {
            q.category
                .as_ref()
                .and_then(|n| cats.iter().find(|c| c.name == *n).map(|c| c.id))
        })
        .or_else(|| cats.iter().find(|c| !c.builtin).map(|c| c.id))
        .or_else(|| cats.first().map(|c| c.id));
    let Some(cat_id) = cat_id else {
        return bad_request("还没有任何分类，请先新建一个");
    };
    let default_name = url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()))
        .unwrap_or_else(|| url.clone());
    let name = if q.name.trim().is_empty() {
        default_name
    } else {
        q.name.trim().to_string()
    };
    let new = db::NewTag {
        category_id: cat_id,
        name: &name,
        url: &url,
        description: "",
        icon: "",
        icon_mime: "",
    };
    let id = match db::create_tag(&st.db, &new) {
        Ok(id) => id,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let st2 = st.clone();
    let url2 = url.clone();
    tokio::spawn(async move {
        favicon::fetch_for_tag(&st2.http, &st2.db, id, &url2, &st2.cfg.icons_dir()).await;
    });
    ok(json!({ "id": id, "name": name, "category_id": cat_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_url_adds_scheme() {
        assert_eq!(normalize_url("baidu.com"), "https://baidu.com");
        assert_eq!(normalize_url("  https://a.com/x  "), "https://a.com/x");
        assert_eq!(normalize_url("http://a.com"), "http://a.com");
        assert_eq!(normalize_url(""), "");
    }

    #[test]
    fn valid_url_rejects_junk() {
        assert!(valid_url("https://a.com"));
        assert!(valid_url("http://127.0.0.1:9000"));
        assert!(!valid_url(""));
        assert!(!valid_url("javascript:alert(1)"));
        assert!(!valid_url("file:///etc/passwd"));
        assert!(!valid_url("ftp://a.com"));
        assert!(!valid_url("https://"));
    }

    #[test]
    fn bookmark_folder_and_links_parsed() {
        let html = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<DL><p>
<DT><H3>技术</H3>
<DL><p>
<DT><A HREF="https://www.rust-lang.org">Rust</A>
<DT><A HREF="https://docs.rs">docs.rs</A>
</DL><p>
<DT><H3>生活</H3>
<DL><p>
<DT><A HREF="https://example.com/">Example</A>
</DL><p>
</DL><p>"#;
        let parsed = parse_bookmarks(html).expect("should parse");
        let names: Vec<&str> = parsed.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"技术"), "got {names:?}");
        assert!(names.contains(&"生活"), "got {names:?}");

        let tech = parsed.iter().find(|(n, _)| n == "技术").unwrap();
        assert_eq!(tech.1.len(), 2);
        assert_eq!(tech.1[0].0, "Rust");
        assert_eq!(tech.1[0].1, "https://www.rust-lang.org");

        let life = parsed.iter().find(|(n, _)| n == "生活").unwrap();
        assert_eq!(life.1[0].1, "https://example.com/");
    }

    #[test]
    fn bookmark_entities_unescaped() {
        let html = r#"<DL><p><DT><H3>A &amp; B</H3><DL><p>
<DT><A HREF="https://x.com?a=1&amp;b=2">X &lt;tag&gt;</A></DL><p></DL><p>"#;
        let parsed = parse_bookmarks(html).expect("should parse");
        let folder = &parsed[0];
        assert_eq!(folder.0, "A & B");
        assert_eq!(folder.1[0].0, "X <tag>");
        assert_eq!(folder.1[0].1, "https://x.com?a=1&b=2");
    }

    #[test]
    fn empty_bookmarks_return_none() {
        assert!(parse_bookmarks("<html><body>nothing</body></html>").is_none());
        assert!(parse_bookmarks("").is_none());
    }

    #[test]
    fn search_engine_must_have_placeholder() {
        assert!(valid_engine("https://www.bing.com/search?q=%s"));
        assert!(!valid_engine("https://www.bing.com/search?q="));
        assert!(!valid_engine("ftp://x/%s"));
        assert!(!valid_engine(""));
    }

    #[test]
    fn cookie_parsed_from_header() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; tab_session=abc123; b=2"),
        );
        assert_eq!(cookie_of(&h, "tab_session").as_deref(), Some("abc123"));
        assert_eq!(cookie_of(&h, "missing"), None);
    }
}
