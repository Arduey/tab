//! favicon 抓取与本地缓存。
//! 策略：先用 Google/DDG 之外的直连方式抓 —— 目标站点自身的 /favicon.ico 与
//! 首页 HTML 里的 <link rel=icon>，失败再退回站点根 favicon.ico。
//! 抓到的图片按 tag id 落到 data/icons/<id>，之后由 /api/favicon/<id> 直接读文件，
//! 不走网络，所以首页加载没有外部依赖。

use crate::db::{self, Db};
use std::path::{Path, PathBuf};
use std::time::Duration;

const UA: &str = "Mozilla/5.0 (compatible; TabIconBot/1.0)";
const MAX_BYTES: usize = 512 * 1024;

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(8))
        .connect_timeout(Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::limited(4))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 探测可用的图片类型（按魔术字节）
pub fn sniff(content_type: Option<&str>, bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 4 {
        return None;
    }
    let m = bytes;
    if m.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if m.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if m.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    if m.starts_with(b"RIFF") && m.len() > 12 && &m[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if m.starts_with(&[0, 0, 1, 0]) {
        return Some("image/x-icon");
    }
    if let Some(ct) = content_type {
        let ct = ct.to_ascii_lowercase();
        if ct.contains("svg") && std::str::from_utf8(m).is_ok() {
            return Some("image/svg+xml");
        }
    }
    None
}

async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Option<(Vec<u8>, Option<String>)> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body = resp.bytes().await.ok()?;
    if body.len() > MAX_BYTES || body.is_empty() {
        return None;
    }
    Some((body.to_vec(), ct))
}

/// 从 HTML 里解析 <link rel="...icon..." href="...">
fn find_icon_links(html: &str, base: &url::Url) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut idx = 0;
    while let Some(pos) = lower[idx..].find("<link") {
        let start = idx + pos;
        let end = match lower[start..].find('>') {
            Some(e) => start + e,
            None => break,
        };
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];
        let rel_ok = tag_lower.contains("icon")
            && (tag_lower.contains("rel=\"shortcut")
                || tag_lower.contains("rel=\"icon")
                || tag_lower.contains("rel=\"apple-touch")
                || tag_lower.contains("rel='icon")
                || tag_lower.contains("rel='shortcut")
                || tag_lower.contains("apple-touch-icon"));
        if rel_ok {
            if let Some(href) = attr_value(tag, "href") {
                if let Ok(u) = base.join(href.trim()) {
                    if u.scheme() == "http" || u.scheme() == "https" {
                        out.push(u.to_string());
                    }
                }
            }
        }
        idx = end + 1;
    }
    // 越大越靠前越可能是高清图标：优先 apple-touch/png
    out.dedup();
    out
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
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

/// 抓取失败时的候选清单
fn candidates(url: &str) -> Option<Vec<String>> {
    let u = url::Url::parse(url).ok()?;
    let origin = format!(
        "{}://{}",
        u.scheme(),
        u.host_str().map(|h| match u.port() {
            Some(p) => format!("{h}:{p}"),
            None => h.to_string(),
        })?
    );
    Some(vec![
        format!("{origin}/favicon.ico"),
        format!("{origin}/favicon.png"),
        format!("{origin}/apple-touch-icon.png"),
    ])
}

/// 给一个标签抓图标，成功返回落盘的相对标识（"favicon"）
pub async fn fetch_for_tag(
    client: &reqwest::Client,
    db: &Db,
    tag_id: i64,
    site_url: &str,
    icons_dir: &Path,
) -> Option<String> {
    let mut tried: Vec<String> = Vec::new();

    // 1) 先看首页 HTML 里声明的图标
    if let Some((body, ct)) = fetch_bytes(client, site_url).await {
        let is_html = ct
            .as_deref()
            .map(|c| c.contains("html"))
            .unwrap_or(false)
            || body.starts_with(b"<")
            || String::from_utf8_lossy(&body[..body.len().min(512)])
                .trim_start()
                .starts_with('<');
        if is_html {
            let html = String::from_utf8_lossy(&body).into_owned();
            if let Ok(base) = url::Url::parse(site_url) {
                for link in find_icon_links(&html, &base).into_iter().take(4) {
                    if tried.contains(&link) {
                        continue;
                    }
                    tried.push(link.clone());
                    if let Some((img, ict)) = fetch_bytes(client, &link).await {
                        if let Some(mime) = sniff(ict.as_deref(), &img) {
                            if save(icons_dir, tag_id, &img).await.is_ok() {
                                let _ = db::set_tag_icon(db, tag_id, "favicon", mime);
                                return Some("favicon".into());
                            }
                        }
                    }
                }
            }
        } else if let Some(mime) = sniff(ct.as_deref(), &body) {
            // 链接直接指向图片
            if save(icons_dir, tag_id, &body).await.is_ok() {
                let _ = db::set_tag_icon(db, tag_id, "favicon", mime);
                return Some("favicon".into());
            }
        }
    }

    // 2) 常规路径
    for cand in candidates(site_url).unwrap_or_default() {
        if tried.contains(&cand) {
            continue;
        }
        if let Some((img, ct)) = fetch_bytes(client, &cand).await {
            if let Some(mime) = sniff(ct.as_deref(), &img) {
                if save(icons_dir, tag_id, &img).await.is_ok() {
                    let _ = db::set_tag_icon(db, tag_id, "favicon", mime);
                    return Some("favicon".into());
                }
            }
        }
    }
    None
}

pub async fn save(dir: &Path, tag_id: i64, bytes: &[u8]) -> std::io::Result<()> {
    let path: PathBuf = dir.join(tag_id.to_string());
    tokio::fs::create_dir_all(dir).await?;
    // 先写临时文件再 rename，避免并发读到半个文件
    let tmp = dir.join(format!("{tag_id}.tmp"));
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, &path).await
}

pub fn icon_path(dir: &Path, tag_id: i64) -> PathBuf {
    dir.join(tag_id.to_string())
}

pub fn remove(dir: &Path, tag_id: i64) {
    let _ = std::fs::remove_file(icon_path(dir, tag_id));
}