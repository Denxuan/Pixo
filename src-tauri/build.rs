fn main() {
    // ScreenCaptureKit's Swift bridge looks for the Xcode toolchain. Command
    // Line Tools keep those compatibility libraries next to the developer dir.
    #[cfg(target_os = "macos")]
    link_command_line_swift();
    tauri_build::build();
}

#[cfg(target_os = "macos")]
fn link_command_line_swift() {
    let Ok(output) = std::process::Command::new("xcode-select")
        .arg("-p")
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let developer = String::from_utf8_lossy(&output.stdout);
    let developer = std::path::Path::new(developer.trim());
    let candidates = [
        developer.join("usr/lib/swift/macosx"),
        developer.join("Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"),
    ];
    for path in candidates {
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}
