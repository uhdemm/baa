fn main() {
    // transcribe.cpp's Vulkan backend links vulkan-1.lib, which lives in the SDK.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-env-changed=VULKAN_SDK");
        if let Ok(sdk) = std::env::var("VULKAN_SDK") {
            println!("cargo:rustc-link-search=native={sdk}\\Lib");
        }
    }
    tauri_build::build()
}
