fn main() {
    hbb_common::gen_version();
    ensure_console_ui_dist();
}

/// The web console embeds `console-ui/dist` (rust-embed). When the UI has not been
/// built, create a placeholder page so the Rust build never fails because of it.
fn ensure_console_ui_dist() {
    println!("cargo:rerun-if-changed=console-ui/dist");
    let dist = std::path::Path::new("console-ui/dist");
    if dist.exists() {
        return;
    }
    let page = "<!doctype html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\"><title>RustDesk Console</title></head>\n<body><p>UI not built - run npm run build in console-ui</p></body>\n</html>\n";
    if let Err(e) = std::fs::create_dir_all(dist).and_then(|_| std::fs::write(dist.join("index.html"), page)) {
        println!("cargo:warning=could not create console-ui/dist placeholder: {}", e);
    }
}
