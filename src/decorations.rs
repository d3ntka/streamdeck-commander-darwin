use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static CACHE: OnceLock<Mutex<HashMap<usize, &'static str>>> = OnceLock::new();

pub fn decorate_menu_icon(svg: &'static str) -> &'static str {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap();
    let key = svg.as_ptr() as usize;
    if let Some(&cached) = map.get(&key) {
        return cached;
    }
    let leaked: &'static str = Box::leak(add_submenu_triangle(svg).into_boxed_str());
    map.insert(key, leaked);
    leaked
}

fn add_submenu_triangle(svg: &str) -> String {
    // Small right-angle triangle in the top-right corner of the 24×24 viewBox.
    // Sits at (20,0)-(24,0)-(24,4): above the folder body (which starts at y≥4).
    // The renderer colorizes all alpha>0 pixels with the foreground color, so the
    // triangle inherits the same tint as the rest of the icon automatically.
    const TRIANGLE: &str = r#"<polygon points="20,0 24,0 24,4"/>"#;
    match svg.rfind("</svg>") {
        Some(pos) => format!("{}{}</svg>", &svg[..pos], TRIANGLE),
        None => svg.to_string(),
    }
}
