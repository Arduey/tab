//! 内置图标库：按名字索引的内联 SVG（stroke 风格），无需联网、无外部依赖。

pub const ICON_NAMES: &[&str] = &[
    "link", "star", "code", "server", "database", "cloud", "home", "book", "video", "music",
    "game", "note", "mail", "chat", "chart", "lock", "settings", "tool", "download", "upload",
    "terminal", "bookmark", "search", "image", "folder", "monitor", "wifi", "globe", "cpu",
    "docker", "git", "api", "fire", "heart", "clock", "calendar", "camera", "bell", "user",
    "shield", "rocket", "cube", "key", "feed", "grid",
];

/// 返回内联 SVG；size 为像素尺寸
pub fn builtin_svg(name: &str, size: u32) -> Option<String> {
    let path = path_of(name)?;
    Some(format!(
        r#"<svg class="svg-icon" width="{s}" height="{s}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{p}</svg>"#,
        s = size,
        p = path
    ))
}

fn path_of(name: &str) -> Option<&'static str> {
    let p = match name {
        "link" => r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#,
        "star" => r#"<path d="m12 3 2.9 5.9 6.5.9-4.7 4.6 1.1 6.5L12 17.8 6.2 20.9l1.1-6.5L2.6 9.8l6.5-.9z"/>"#,
        "code" => r#"<path d="m16 18 6-6-6-6"/><path d="m8 6-6 6 6 6"/>"#,
        "server" => r#"<rect x="2" y="3" width="20" height="7" rx="2"/><rect x="2" y="14" width="20" height="7" rx="2"/><path d="M6 6.5h.01M6 17.5h.01"/>"#,
        "database" => r#"<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.7 4 3 9 3s9-1.3 9-3V5"/><path d="M3 12c0 1.7 4 3 9 3s9-1.3 9-3"/>"#,
        "cloud" => r#"<path d="M17.5 19a4.5 4.5 0 0 0 .5-9 6 6 0 0 0-11.6-1.5A4 4 0 0 0 6.5 19z"/>"#,
        "home" => r#"<path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"/><path d="M9 21v-8h6v8"/>"#,
        "book" => r#"<path d="M4 4h6a3 3 0 0 1 3 3v13a2.5 2.5 0 0 0-2.5-2.5H4z"/><path d="M20 4h-6a3 3 0 0 0-3 3v13a2.5 2.5 0 0 1 2.5-2.5H20z"/>"#,
        "video" => r#"<rect x="2" y="6" width="14" height="12" rx="2"/><path d="m16 12 6-4v8z"/>"#,
        "music" => r#"<path d="M9 18V6l10-2v12"/><circle cx="6" cy="18" r="3"/><circle cx="16" cy="16" r="3"/>"#,
        "game" => r#"<path d="M7 12h4M9 10v4"/><circle cx="16" cy="11" r="1"/><circle cx="18.5" cy="13.5" r="1"/><rect x="2" y="6" width="20" height="12" rx="5"/>"#,
        "note" => r#"<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6"/>"#,
        "mail" => r#"<rect x="2" y="5" width="20" height="14" rx="2"/><path d="m3 7 9 6 9-6"/>"#,
        "chat" => r#"<path d="M21 12a8 8 0 0 1-8 8H8l-5 3 1.5-5A8 8 0 0 1 13 4a8 8 0 0 1 8 8z"/>"#,
        "chart" => r#"<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>"#,
        "lock" => r#"<rect x="4" y="10" width="16" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/>"#,
        "settings" => r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.9 1.2V21a2 2 0 1 1-4 0v-.1A1.7 1.7 0 0 0 7 19.4l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.7 1.7 0 0 0 3 13.8H3a2 2 0 1 1 0-4h.1A1.7 1.7 0 0 0 4.6 7l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.7 1.7 0 0 0 10 3.1V3a2 2 0 1 1 4 0v.1A1.7 1.7 0 0 0 17 4.6l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0 1.2 2.9H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>"#,
        "tool" => r#"<path d="M14.7 6.3a4 4 0 0 1 5 5L9 22l-4-4z"/><path d="m18 2 4 4-2.5 2.5-4-4z"/>"#,
        "download" => r#"<path d="M12 3v12"/><path d="m7 11 5 5 5-5"/><path d="M4 20h16"/>"#,
        "upload" => r#"<path d="M12 20V8"/><path d="m7 12 5-5 5 5"/><path d="M4 3h16"/>"#,
        "terminal" => r#"<rect x="2" y="4" width="20" height="16" rx="2"/><path d="m6 10 3 2-3 2M12 15h5"/>"#,
        "bookmark" => r#"<path d="M6 3h12a1 1 0 0 1 1 1v17l-7-4-7 4V4a1 1 0 0 1 1-1z"/>"#,
        "search" => r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>"#,
        "image" => r#"<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="9" r="1.5"/><path d="m4 18 5-5 4 4 3-3 4 4"/>"#,
        "folder" => r#"<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#,
        "monitor" => r#"<rect x="2" y="4" width="20" height="13" rx="2"/><path d="M8 21h8M12 17v4"/>"#,
        "wifi" => r#"<path d="M2 8a16 16 0 0 1 20 0"/><path d="M5 12a11 11 0 0 1 14 0"/><path d="M8.5 15.5a6 6 0 0 1 7 0"/><circle cx="12" cy="19" r="1"/>"#,
        "globe" => r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a15 15 0 0 1 0 18a15 15 0 0 1 0-18z"/>"#,
        "cpu" => r#"<rect x="6" y="6" width="12" height="12" rx="2"/><rect x="9" y="9" width="6" height="6"/><path d="M12 2v4M12 18v4M2 12h4M18 12h4M5 5l2 2M19 5l-2 2M5 19l2-2M19 19l-2-2"/>"#,
        "docker" => r#"<path d="M3 12h10v4a5 5 0 0 1-5 5H8a5 5 0 0 1-5-5z"/><path d="M6 9h3v3H6zM10 9h3v3h-3zM6 5h3v4H6z"/><path d="M14 12h3a2 2 0 0 0 2-2"/>"#,
        "git" => r#"<circle cx="6" cy="6" r="2.5"/><circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="12" r="2.5"/><path d="M6 8.5v7"/><path d="M8.5 6H14a2 2 0 0 1 2 2v2"/>"#,
        "api" => r#"<path d="M8 4 4 12l4 8"/><path d="m16 4 4 8-4 8"/><path d="M13 4 11 20"/>"#,
        "fire" => r#"<path d="M12 2c4 5 6 7 6 11a6 6 0 1 1-12 0c0-2 1-3.5 2.5-5C9 9.5 12 6 12 2z"/> "#,
        "heart" => r#"<path d="M12 20s-8-4.5-8-10a4.5 4.5 0 0 1 8-2.8A4.5 4.5 0 0 1 20 10c0 5.5-8 10-8 10z"/>"#,
        "clock" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3.5 2"/>"#,
        "calendar" => r#"<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 10h18M8 3v4M16 3v4"/>"#,
        "camera" => r#"<path d="M4 8h3l2-2h6l2 2h3a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z"/><circle cx="12" cy="13" r="3.5"/>"#,
        "bell" => r#"<path d="M18 8a6 6 0 1 0-12 0c0 6-3 7-3 7h18s-3-1-3-7"/><path d="M10.5 20a2 2 0 0 0 3 0"/>"#,
        "user" => r#"<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>"#,
        "shield" => r#"<path d="M12 3 5 6v6c0 4.5 3 7.5 7 9 4-1.5 7-4.5 7-9V6z"/>"#,
        "rocket" => r#"<path d="M14 4c4 1 6 3 6 6l-6 6-4-4z"/><path d="M10 12 4 18l2 2 6-6"/><circle cx="15.5" cy="8.5" r="1"/>"#,
        "cube" => r#"<path d="m12 3 8 4.5v9L12 21l-8-4.5v-9z"/><path d="M4 7.5 12 12l8-4.5M12 12v9"/>"#,
        "key" => r#"<circle cx="8" cy="15" r="4"/><path d="m11 12 9-9M17 3l3 3M14 6l3 3"/>"#,
        "feed" => r#"<path d="M4 11a9 9 0 0 1 9 9M4 4a16 16 0 0 1 16 16"/><circle cx="5" cy="19" r="1.5"/>"#,
        "grid" => r#"<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>"#,
        _ => return None,
    };
    Some(p)
}



/// 供 API 返回给客户端：名字 + 内联 SVG（一次拿全，选择器不用逐个请求）
pub fn catalog() -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for name in ICON_NAMES {
        if let Some(svg) = builtin_svg(name, 20) {
            out.insert((*name).to_string(), serde_json::Value::String(svg));
        }
    }
    serde_json::Value::Object(out)
}