fn main() {
    // 用自定义清单替换 tauri-build 的默认清单：核心差别是
    // requestedExecutionLevel = requireAdministrator，启动时由系统弹 UAC。
    // 清单内容见 app.manifest（保留默认清单里的 Common Controls v6 依赖）。
    println!("cargo:rerun-if-changed=app.manifest");

    // app.manifest 必须是纯 ASCII。tauri-winres 用 rc.exe 编译清单，rc.exe 会用
    // 系统 ANSI 代码页重新编码清单资源：文件里任何非 ASCII 字符（哪怕在注释里）
    // 都会让内嵌清单变成非法 UTF-8，Windows 启动程序时报「并行配置不正确」。
    let manifest = include_str!("app.manifest");
    assert!(
        manifest.is_ascii(),
        "app.manifest must be pure ASCII (no Chinese comments), otherwise the embedded manifest is invalid and the app cannot start"
    );

    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri build script");
}
