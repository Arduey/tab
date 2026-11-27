//! 静态资源：编译期内嵌进二进制，运行时零磁盘 IO、零额外依赖。
//!
//! 缓存策略：`VERSION` 常量拼进 URL（`/static/v3/app.js`），响应带
//! `immutable` 强缓存；ETag 由版本 + 实际内容长度组成。升级二进制时
//! 把 `VERSION` 改掉（或直接依赖内容长度变化）就能让浏览器重新拉取，
//! 不会出现「改了 JS 但用户还跑旧代码」。

pub struct Asset {
    pub body: &'static str,
    pub content_type: &'static str,
}

const CSS: &str = include_str!("../web/static/app.css");
const JS: &str = include_str!("../web/static/app.js");
const MANAGE_JS: &str = include_str!("../web/static/manage.js");
const LOGO: &str = include_str!("../web/static/logo.svg");

/// 资源版本号：前端文件有实质改动时改这里。
/// 注意：模板里 `/static/v3/...` 的 v3 必须与此保持一致
/// （模板是编译期字符串，没法直接插变量）。
pub const VERSION: &str = "3";

pub fn get(name: &str) -> Option<Asset> {
    let (body, content_type) = match name {
        "app.css" => (CSS, "text/css; charset=utf-8"),
        "app.js" => (JS, "application/javascript; charset=utf-8"),
        "manage.js" => (MANAGE_JS, "application/javascript; charset=utf-8"),
        "logo.svg" => (LOGO, "image/svg+xml"),
        _ => return None,
    };
    Some(Asset { body, content_type })
}

/// ETag 需要在运行期拼（长度来自实际内容），调用方拿到 String
pub fn etag(name: &str) -> String {
    match get(name) {
        Some(a) => format!("{}-{}-{}", VERSION, name, a.body.len()),
        None => format!("{}-unknown", VERSION),
    }
}