use std::process::Command;

fn main() {
    let output = Command::new("date")
        .args(["+%Y-%m-%d %H:%M:%S"])
        .output();
    let build_time = output
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_time);
}
