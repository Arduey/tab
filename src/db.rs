//! SQLite 层：schema、迁移、以及所有查询/写入。
//! 用 r2d2 连接池 + WAL，读多写少场景下几乎无锁等待。

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub type Db = Pool<SqliteConnectionManager>;

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS admin (
  id            INTEGER PRIMARY KEY CHECK (id = 1),
  username      TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS categories (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL,
  icon       TEXT NOT NULL DEFAULT '',
  color      TEXT NOT NULL DEFAULT '',
  sort       INTEGER NOT NULL DEFAULT 0,
  builtin    INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  category_id  INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  url          TEXT NOT NULL,
  description  TEXT NOT NULL DEFAULT '',
  icon         TEXT NOT NULL DEFAULT '',
  icon_mime    TEXT NOT NULL DEFAULT '',
  sort         INTEGER NOT NULL DEFAULT 0,
  hits         INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tags_cat ON tags(category_id, sort);
CREATE INDEX IF NOT EXISTS idx_tags_sort ON tags(sort);

CREATE TABLE IF NOT EXISTS api_tokens (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  scope      TEXT NOT NULL DEFAULT 'read,write',
  last_used  INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
  id         TEXT PRIMARY KEY,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  user_agent TEXT NOT NULL DEFAULT ''
);
"#;

// ---------- 模型 ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub sort: i64,
    pub builtin: bool,
    pub count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Tag {
    pub id: i64,
    pub category_id: i64,
    pub name: String,
    pub url: String,
    pub description: String,
    /// 图标：空=无；"favicon"=自动缓存的 favicon；"builtin:xxx"=内置图标；"upload:文件名"=上传
    pub icon: String,
    pub icon_mime: String,
    pub sort: i64,
    pub hits: i64,
    pub category_name: Option<String>,
}

pub fn open(path: &Path) -> anyhow::Result<Db> {
    let manager = SqliteConnectionManager::file(path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;
             PRAGMA temp_store=MEMORY;
             PRAGMA mmap_size=134217728;",
        )
    });
    let pool = Pool::builder()
        .max_size(8)
        .min_idle(Some(1))
        .build(manager)?;
    let conn = pool.get()?;
    conn.execute_batch(SCHEMA)?;
    Ok(pool)
}

pub fn is_installed(db: &Db) -> bool {
    let conn = match db.get() {
        Ok(c) => c,
        Err(_) => return false,
    };
    conn.query_row("SELECT COUNT(*) FROM admin", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0)
        > 0
}

// ---------- 设置 ----------


pub fn set_setting(db: &Db, key: &str, value: &str) -> anyhow::Result<()> {
    let conn = db.get()?;
    conn.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn all_settings(db: &Db) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Ok(conn) = db.get() {
        if let Ok(mut stmt) = conn.prepare("SELECT key,value FROM settings") {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    map.insert(row.0, row.1);
                }
            }
        }
    }
    map
}

// ---------- 分类 ----------

pub fn list_categories(db: &Db) -> Vec<Category> {
    let conn = match db.get() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare(
        "SELECT c.id,c.name,c.icon,c.color,c.sort,c.builtin,
                (SELECT COUNT(*) FROM tags t WHERE t.category_id=c.id)
         FROM categories c ORDER BY c.builtin DESC, c.sort ASC, c.id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map([], |r| {
        Ok(Category {
            id: r.get(0)?,
            name: r.get(1)?,
            icon: r.get(2)?,
            color: r.get(3)?,
            sort: r.get(4)?,
            builtin: r.get::<_, i64>(5)? != 0,
            count: r.get(6)?,
        })
    });
    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => vec![],
    }
}

pub fn create_category(db: &Db, name: &str, icon: &str, color: &str) -> anyhow::Result<i64> {
    let conn = db.get()?;
    let next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort),0)+10 FROM categories WHERE builtin=0",
        [],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO categories(name,icon,color,sort,builtin,created_at) VALUES(?1,?2,?3,?4,0,?5)",
        params![name, icon, color, next, crate::now()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_category(
    db: &Db,
    id: i64,
    name: &str,
    icon: &str,
    color: &str,
) -> anyhow::Result<()> {
    let conn = db.get()?;
    conn.execute(
        "UPDATE categories SET name=?2, icon=?3, color=?4 WHERE id=?1",
        params![id, name, icon, color],
    )?;
    Ok(())
}

pub fn delete_category(db: &Db, id: i64) -> anyhow::Result<bool> {
    let conn = db.get()?;
    let builtin: i64 = conn
        .query_row("SELECT builtin FROM categories WHERE id=?1", params![id], |r| {
            r.get(0)
        })
        .optional()?
        .unwrap_or(0);
    if builtin != 0 {
        return Ok(false);
    }
    conn.execute("DELETE FROM categories WHERE id=?1", params![id])?;
    Ok(true)
}

/// 拖拽排序：categories 顺序
pub fn reorder_categories(db: &Db, ids: &[i64]) -> anyhow::Result<()> {
    let mut conn = db.get()?;
    let tx = conn.transaction()?;
    for (i, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE categories SET sort=?2 WHERE id=?1 AND builtin=0",
            params![id, (i as i64 + 1) * 10],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// ---------- 标签 ----------

pub fn list_tags(db: &Db) -> Vec<Tag> {
    let conn = match db.get() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare(
        "SELECT t.id,t.category_id,t.name,t.url,t.description,t.icon,t.icon_mime,t.sort,t.hits,c.name
         FROM tags t LEFT JOIN categories c ON c.id=t.category_id
         ORDER BY t.sort ASC, t.id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map([], |r| {
        Ok(Tag {
            id: r.get(0)?,
            category_id: r.get(1)?,
            name: r.get(2)?,
            url: r.get(3)?,
            description: r.get(4)?,
            icon: r.get(5)?,
            icon_mime: r.get(6)?,
            sort: r.get(7)?,
            hits: r.get(8)?,
            category_name: r.get(9)?,
        })
    });
    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => vec![],
    }
}

pub struct NewTag<'a> {
    pub category_id: i64,
    pub name: &'a str,
    pub url: &'a str,
    pub description: &'a str,
    pub icon: &'a str,
    pub icon_mime: &'a str,
}

pub fn create_tag(db: &Db, t: &NewTag<'_>) -> anyhow::Result<i64> {
    let conn = db.get()?;
    let next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort),0)+10 FROM tags WHERE category_id=?1",
        params![t.category_id],
        |r| r.get(0),
    )?;
    let ts = crate::now();
    conn.execute(
        "INSERT INTO tags(category_id,name,url,description,icon,icon_mime,sort,hits,created_at,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,0,?8,?8)",
        params![
            t.category_id,
            t.name,
            t.url,
            t.description,
            t.icon,
            t.icon_mime,
            next,
            ts
        ],
    )?;
    Ok(conn.last_insert_rowid())
}


pub fn set_tag_icon(db: &Db, id: i64, icon: &str, mime: &str) -> anyhow::Result<()> {
    let conn = db.get()?;
    conn.execute(
        "UPDATE tags SET icon=?2, icon_mime=?3, updated_at=?4 WHERE id=?1",
        params![id, icon, mime, crate::now()],
    )?;
    Ok(())
}

pub fn update_tag(
    db: &Db,
    id: i64,
    category_id: i64,
    name: &str,
    url: &str,
    description: &str,
    icon: &str,
) -> anyhow::Result<()> {
    let conn = db.get()?;
    conn.execute(
        "UPDATE tags SET category_id=?2,name=?3,url=?4,description=?5,icon=?6,updated_at=?7 WHERE id=?1",
        params![id, category_id, name, url, description, icon, crate::now()],
    )?;
    Ok(())
}

pub fn delete_tag(db: &Db, id: i64) -> anyhow::Result<()> {
    let conn = db.get()?;
    conn.execute("DELETE FROM tags WHERE id=?1", params![id])?;
    Ok(())
}

pub fn bump_hit(db: &Db, id: i64) {
    if let Ok(conn) = db.get() {
        let _ = conn.execute("UPDATE tags SET hits=hits+1 WHERE id=?1", params![id]);
    }
}

/// 去重键：忽略协议、开头的 www.、末尾斜杠与 fragment，尽量判定为同一站点
pub fn url_key(u: &str) -> String {
    let lower = u.trim().to_ascii_lowercase();
    let no_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower);
    let no_frag = no_scheme.split('#').next().unwrap_or(no_scheme);
    let trimmed = no_frag.trim_end_matches('/');
    let no_www = trimmed.strip_prefix("www.").unwrap_or(trimmed);
    no_www.to_string()
}

pub fn find_tag_by_url(db: &Db, url: &str) -> Option<i64> {
    let conn = db.get().ok()?;
    // 先按原样精确命中，再按规范化键退一步比较，避免 baidu.com 与 www.baidu.com 重复
    let exact: Option<i64> = conn
        .query_row("SELECT id FROM tags WHERE url=?1", params![url], |r| r.get(0))
        .optional()
        .ok()
        .flatten();
    if exact.is_some() {
        return exact;
    }
    let key = url_key(url);
    let mut stmt = conn.prepare("SELECT id, url FROM tags").ok()?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .ok()?;
    for row in rows.flatten() {
        if url_key(&row.1) == key {
            return Some(row.0);
        }
    }
    None
}

/// 拖拽排序：一次性写入多个标签的 (category_id, sort)
pub fn reorder_tags(db: &Db, items: &[(i64, i64, i64)]) -> anyhow::Result<()> {
    let mut conn = db.get()?;
    let tx = conn.transaction()?;
    for (tag_id, category_id, sort) in items {
        tx.execute(
            "UPDATE tags SET category_id=?2, sort=?3, updated_at=?4 WHERE id=?1",
            params![tag_id, category_id, sort, crate::now()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// ---------- 管理员 ----------

pub fn create_admin(db: &Db, username: &str, hash: &str) -> anyhow::Result<()> {
    let conn = db.get()?;
    conn.execute(
        "INSERT INTO admin(id,username,password_hash,created_at) VALUES(1,?1,?2,?3)",
        params![username, hash, crate::now()],
    )?;
    Ok(())
}

pub fn admin_credentials(db: &Db) -> Option<(String, String)> {
    let conn = db.get().ok()?;
    conn.query_row("SELECT username,password_hash FROM admin WHERE id=1", [], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })
    .optional()
    .ok()
    .flatten()
}

pub fn update_admin(db: &Db, username: Option<&str>, hash: Option<&str>) -> anyhow::Result<()> {
    let conn = db.get()?;
    if let Some(u) = username {
        conn.execute("UPDATE admin SET username=?1 WHERE id=1", params![u])?;
    }
    if let Some(h) = hash {
        conn.execute("UPDATE admin SET password_hash=?1 WHERE id=1", params![h])?;
    }
    Ok(())
}

// ---------- 会话 ----------

pub fn create_session(db: &Db, id: &str, ttl: i64) {
    if let Ok(conn) = db.get() {
        let now = crate::now();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO sessions(id,created_at,expires_at) VALUES(?1,?2,?3)",
            params![id, now, now + ttl],
        );
        let _ = conn.execute("DELETE FROM sessions WHERE expires_at<?1", params![now]);
    }
}

pub fn session_valid(db: &Db, id: &str) -> bool {
    let Ok(conn) = db.get() else { return false };
    conn.query_row(
        "SELECT expires_at FROM sessions WHERE id=?1",
        params![id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
    .map(|exp| exp > crate::now())
    .unwrap_or(false)
}

pub fn destroy_session(db: &Db, id: &str) {
    if let Ok(conn) = db.get() {
        let _ = conn.execute("DELETE FROM sessions WHERE id=?1", params![id]);
    }
}

pub fn destroy_all_sessions(db: &Db, keep: Option<&str>) {
    if let Ok(conn) = db.get() {
        match keep {
            Some(k) => {
                let _ = conn.execute("DELETE FROM sessions WHERE id<>?1", params![k]);
            }
            None => {
                let _ = conn.execute("DELETE FROM sessions", []);
            }
        }
    }
}

// ---------- API token ----------

pub fn create_token(db: &Db, name: &str, scope: &str) -> anyhow::Result<String> {
    use rand::RngCore;
    let mut raw = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = format!("tab_{}", hex::encode(raw));
    let conn = db.get()?;
    conn.execute(
        "INSERT INTO api_tokens(name,token_hash,scope,last_used,created_at) VALUES(?1,?2,?3,0,?4)",
        params![name, crate::auth::sha256_hex(&token), scope, crate::now()],
    )?;
    Ok(token)
}

pub fn list_tokens(db: &Db) -> Vec<serde_json::Value> {
    let Ok(conn) = db.get() else { return vec![] };
    let Ok(mut stmt) = conn.prepare("SELECT id,name,scope,last_used,created_at FROM api_tokens ORDER BY id DESC") else {
        return vec![];
    };
    let rows = stmt.query_map([], |r| {
        Ok(serde_json::json!({
            "id": r.get::<_,i64>(0)?,
            "name": r.get::<_,String>(1)?,
            "scope": r.get::<_,String>(2)?,
            "last_used": r.get::<_,i64>(3)?,
            "created_at": r.get::<_,i64>(4)?,
        }))
    });
    rows.map(|r| r.flatten().collect()).unwrap_or_default()
}

pub fn delete_token(db: &Db, id: i64) {
    if let Ok(conn) = db.get() {
        let _ = conn.execute("DELETE FROM api_tokens WHERE id=?1", params![id]);
    }
}

/// 校验 token，返回 scope
pub fn check_token(db: &Db, raw: &str) -> Option<String> {
    let conn = db.get().ok()?;
    let hash = crate::auth::sha256_hex(raw);
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT id,scope FROM api_tokens WHERE token_hash=?1",
            params![hash],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .ok()
        .flatten();
    let (id, scope) = row?;
    // 每分钟最多更新一次 last_used，避免写放大
    let _ = conn.execute(
        "UPDATE api_tokens SET last_used=?1 WHERE id=?2 AND last_used<?1-60",
        params![crate::now(), id],
    );
    Some(scope)
}
// ---------- 补充查询 ----------

pub fn find_category_by_name(db: &Db, name: &str) -> Option<i64> {
    let conn = db.get().ok()?;
    conn.query_row(
        "SELECT id FROM categories WHERE name=?1 LIMIT 1",
        params![name],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn tag_icon_mime(db: &Db, id: i64) -> Option<String> {
    let conn = db.get().ok()?;
    let mime: Option<String> = conn
        .query_row("SELECT icon_mime FROM tags WHERE id=?1", params![id], |r| {
            r.get(0)
        })
        .optional()
        .ok()
        .flatten();
    mime.filter(|m| !m.is_empty())
}

/// 清空所有分类与标签（保留内置 Docker 栏与账号）
pub fn wipe_all(db: &Db) -> anyhow::Result<()> {
    let mut conn = db.get()?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM tags", [])?;
    tx.execute("DELETE FROM categories WHERE builtin=0", [])?;
    tx.commit()?;
    Ok(())
}

/// 前端需要的设置项
pub fn public_settings(db: &Db) -> serde_json::Value {
    let map = all_settings(db);
    let get = |k: &str, d: &str| map.get(k).cloned().unwrap_or_else(|| d.to_string());
    serde_json::json!({
        "site_title": get("site_title", "我的导航"),
        "theme": get("theme", "auto"),
        "accent": get("accent", "#3b82f6"),
        "open_in_new_tab": get("open_in_new_tab", "1") == "1",
        "search_engine": get("search_engine", "https://www.google.com/search?q=%s"),
        "show_categories": get("show_categories", "1") == "1",
    })
}

/// 事务包装：多条写入合并成一次 fsync，批量导入时明显减少抖动
pub fn with_tx<T>(db: &Db, f: impl FnOnce(&rusqlite::Transaction<'_>) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let mut conn = db.get()?;
    let tx = conn.transaction()?;
    let out = f(&tx)?;
    tx.commit()?;
    Ok(out)
}

/// 批量插入标签，返回 (新增数, 跳过数)
pub fn bulk_insert_tags(db: &Db, items: &[(i64, String, String, String, String)]) -> (usize, usize) {
    let mut created = 0;
    let mut skipped = 0;
    let res = with_tx(db, |tx| {
        let ts = crate::now();
        // 已存在的规范化键集合，一次读出来，避免逐条全表扫
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        {
            let mut stmt = tx.prepare("SELECT url FROM tags")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            for row in rows.flatten() {
                seen.insert(url_key(&row));
            }
        }
        for (cat_id, name, url, icon, desc) in items {
            let key = url_key(url);
            if seen.contains(&key) {
                skipped += 1;
                continue;
            }
            seen.insert(key);
            let next: i64 = tx.query_row(
                "SELECT COALESCE(MAX(sort),0)+10 FROM tags WHERE category_id=?1",
                params![cat_id],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO tags(category_id,name,url,description,icon,icon_mime,sort,hits,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,'',?6,0,?7,?7)",
                params![cat_id, name, url, desc, icon, next, ts],
            )?;
            created += 1;
        }
        Ok(())
    });
    if res.is_err() {
        return (0, items.len());
    }
    (created, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Db {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory().with_init(|c| {
            c.execute_batch("PRAGMA foreign_keys=ON;")
        });
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        pool.get().unwrap().execute_batch(SCHEMA).unwrap();
        pool
    }

    #[test]
    fn url_key_ignores_scheme_www_and_slash() {
        assert_eq!(url_key("https://www.baidu.com/"), "baidu.com");
        assert_eq!(url_key("http://baidu.com"), "baidu.com");
        assert_eq!(url_key("https://BAIDU.com/#top"), "baidu.com");
        // 路径不同的仍是不同站点
        assert_ne!(url_key("https://a.com/x"), url_key("https://a.com/y"));
    }

    #[test]
    fn duplicate_detection_by_normalized_key() {
        let db = memory_db();
        let cat = create_category(&db, "常用", "", "").unwrap();
        let new = NewTag {
            category_id: cat,
            name: "Baidu",
            url: "https://www.baidu.com",
            description: "",
            icon: "",
            icon_mime: "",
        };
        let id = create_tag(&db, &new).unwrap();
        // www 与不带 www、https 与 http 都应命中同一条
        assert_eq!(find_tag_by_url(&db, "https://www.baidu.com"), Some(id));
        assert_eq!(find_tag_by_url(&db, "https://baidu.com/"), Some(id));
        assert_eq!(find_tag_by_url(&db, "http://baidu.com"), Some(id));
        assert_eq!(find_tag_by_url(&db, "https://other.com"), None);
    }

    #[test]
    fn builtin_category_cannot_be_deleted() {
        let db = memory_db();
        let conn = db.get().unwrap();
        conn.execute(
            "INSERT INTO categories(name,icon,color,sort,builtin,created_at) VALUES('Docker','docker','',0,1,0)",
            [],
        )
        .unwrap();
        let docker_id: i64 = conn
            .query_row("SELECT id FROM categories WHERE builtin=1", [], |r| r.get(0))
            .unwrap();
        drop(conn);
        assert!(!delete_category(&db, docker_id).unwrap());

        let normal = create_category(&db, "临时", "", "").unwrap();
        assert!(delete_category(&db, normal).unwrap());
    }

    #[test]
    fn deleting_category_cascades_tags() {
        let db = memory_db();
        let cat = create_category(&db, "会删掉", "", "").unwrap();
        let id = create_tag(
            &db,
            &NewTag {
                category_id: cat,
                name: "X",
                url: "https://x.com",
                description: "",
                icon: "",
                icon_mime: "",
            },
        )
        .unwrap();
        assert_eq!(list_tags(&db).len(), 1);
        delete_category(&db, cat).unwrap();
        assert!(list_tags(&db).is_empty(), "标签应随分类级联删除");
        assert_eq!(find_tag_by_url(&db, "https://x.com"), None);
        let _ = id;
    }

    #[test]
    fn bulk_insert_skips_duplicates_in_one_pass() {
        let db = memory_db();
        let cat = create_category(&db, "批量", "", "").unwrap();
        let items = vec![
            (cat, "A".into(), "https://a.com".into(), String::new(), String::new()),
            // 与上面同一个站点的不同写法，应被跳过
            (cat, "A2".into(), "http://www.a.com/".into(), String::new(), String::new()),
            (cat, "B".into(), "https://b.com".into(), String::new(), String::new()),
        ];
        let (created, skipped) = bulk_insert_tags(&db, &items);
        assert_eq!(created, 2);
        assert_eq!(skipped, 1);
        assert_eq!(list_tags(&db).len(), 2);
    }

    #[test]
    fn tags_sorted_within_category() {
        let db = memory_db();
        let cat = create_category(&db, "C", "", "").unwrap();
        for (n, u) in [("1", "https://1.com"), ("2", "https://2.com"), ("3", "https://3.com")] {
            create_tag(
                &db,
                &NewTag {
                    category_id: cat,
                    name: n,
                    url: u,
                    description: "",
                    icon: "",
                    icon_mime: "",
                },
            )
            .unwrap();
        }
        let names: Vec<String> = list_tags(&db).into_iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["1", "2", "3"]);
    }

    #[test]
    fn reorder_moves_tags_between_categories() {
        let db = memory_db();
        let a = create_category(&db, "A", "", "").unwrap();
        let b = create_category(&db, "B", "", "").unwrap();
        let id = create_tag(
            &db,
            &NewTag {
                category_id: a,
                name: "T",
                url: "https://t.com",
                description: "",
                icon: "",
                icon_mime: "",
            },
        )
        .unwrap();
        reorder_tags(&db, &[(id, b, 10)]).unwrap();
        let tag = list_tags(&db).into_iter().find(|t| t.id == id).unwrap();
        assert_eq!(tag.category_id, b);
        assert_eq!(tag.sort, 10);
    }

    #[test]
    fn session_lifecycle() {
        let db = memory_db();
        create_session(&db, "sid-1", 3600);
        assert!(session_valid(&db, "sid-1"));
        assert!(!session_valid(&db, "nope"));
        destroy_session(&db, "sid-1");
        assert!(!session_valid(&db, "sid-1"));

        // 已过期的会话不算有效
        create_session(&db, "old", -10);
        assert!(!session_valid(&db, "old"));
    }

    #[test]
    fn api_token_roundtrip() {
        let db = memory_db();
        let raw = create_token(&db, "cli", "read").unwrap();
        assert!(raw.starts_with("tab_"));
        assert_eq!(check_token(&db, &raw).as_deref(), Some("read"));
        assert_eq!(check_token(&db, "tab_wrong"), None);

        let id = list_tokens(&db)[0]["id"].as_i64().unwrap();
        delete_token(&db, id);
        assert_eq!(check_token(&db, &raw), None);
    }

    #[test]
    fn token_stored_hashed_not_plaintext() {
        let db = memory_db();
        let raw = create_token(&db, "cli", "read,write").unwrap();
        let conn = db.get().unwrap();
        let stored: String = conn
            .query_row("SELECT token_hash FROM api_tokens", [], |r| r.get(0))
            .unwrap();
        assert_ne!(stored, raw, "数据库里不能存明文令牌");
        assert_eq!(stored, crate::auth::sha256_hex(&raw));
    }

    #[test]
    fn wipe_keeps_builtin_category() {
        let db = memory_db();
        let conn = db.get().unwrap();
        conn.execute(
            "INSERT INTO categories(name,icon,color,sort,builtin,created_at) VALUES('Docker','docker','',0,1,0)",
            [],
        )
        .unwrap();
        drop(conn);
        let cat = create_category(&db, "普通", "", "").unwrap();
        create_tag(
            &db,
            &NewTag {
                category_id: cat,
                name: "X",
                url: "https://x.com",
                description: "",
                icon: "",
                icon_mime: "",
            },
        )
        .unwrap();

        wipe_all(&db).unwrap();
        let cats = list_categories(&db);
        assert_eq!(cats.len(), 1);
        assert!(cats[0].builtin, "内置 Docker 栏必须保留");
        assert!(list_tags(&db).is_empty());
    }

    #[test]
    fn category_count_reflects_tags() {
        let db = memory_db();
        let cat = create_category(&db, "统计", "", "").unwrap();
        assert_eq!(list_categories(&db)[0].count, 0);
        create_tag(
            &db,
            &NewTag {
                category_id: cat,
                name: "X",
                url: "https://x.com",
                description: "",
                icon: "",
                icon_mime: "",
            },
        )
        .unwrap();
        assert_eq!(list_categories(&db)[0].count, 1);
    }
}
