fn main() {
    // Link AVFoundation for AVCaptureDevice microphone permission check
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=AVFoundation");

    tauri_build::build()
}
