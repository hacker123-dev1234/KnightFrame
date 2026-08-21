use std::{env, fs, path::PathBuf};

fn embed_directory(directory: PathBuf, extension: &str, output_name: &str, const_name: &str) {
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut files = fs::read_dir(&directory)
        .unwrap_or_else(|_| panic!("{output_name} directory"))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect::<Vec<_>>();
    files.sort();
    let entries = files
        .iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            format!(
                "({name:?}, include_str!({:?})),",
                path.display().to_string()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let output = PathBuf::from(env::var("OUT_DIR").expect("build output")).join(output_name);
    fs::write(
        output,
        format!("pub const {const_name}: &[(&str, &str)] = &[\n{entries}\n];\n"),
    )
    .unwrap_or_else(|_| panic!("write embedded {output_name}"));
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    embed_directory(
        manifest.join("skills/builtin"),
        "md",
        "builtin_skills.rs",
        "BUILTIN_SKILLS",
    );
    embed_directory(
        manifest.join("market_prompts"),
        "txt",
        "market_prompts.rs",
        "MARKET_PROMPTS",
    );
    // 关闭 tauri_build 的默认 app manifest（它把 RT_MANIFEST 编进 resource.lib，
    // 只有链接 resource.lib 的产物能拿到；且与下面的链接器嵌入叠加会 CVT1100 重复）。
    tauri_build::try_build(
        tauri_build::Attributes::default()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("tauri build");
    embed_linker_manifest();
}

/// 通过链接器给**所有**产物（exe / cdyll / cargo 测试二进制）统一嵌入 SxS
/// manifest（Common-Controls 6.0）：tao 静态导入 comctl32!TaskDialogIndirect，
/// 无 manifest 时加载器回退 comctl32 5.x，启动即 0xC0000139。
/// cargo 没有"仅测试二进制"的 link-arg 指令，所以必须全局嵌入，
/// 同时用 new_without_app_manifest 防止 resource.lib 里重复。
fn embed_linker_manifest() {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0" processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;
    let out = PathBuf::from(env::var("OUT_DIR").expect("build output"));
    let path = out.join("knightframe.sxs.manifest");
    std::fs::write(&path, MANIFEST).expect("write sxs manifest");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTUAC:NO");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", path.display());
}
