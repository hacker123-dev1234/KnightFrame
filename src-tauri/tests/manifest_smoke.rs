//! 进程级冒烟：测试二进制必须携带 SxS manifest（声明 Common-Controls 6.0），
//! 否则加载器回退到 comctl32 5.x，tao 的 TaskDialogIndirect 入口缺失会让
//! 任何测试在加载期以 STATUS_ENTRYPOINT_NOT_FOUND (0xC0000139) 退出。
//! 本测试能被收集并运行，即证明 manifest 生效。

#[test]
fn process_boots_with_side_by_side_manifest() {
    let embedded = option_env!("CARGO_CFG_TARGET_ENV");
    assert!(
        embedded.is_none_or(|value| value != "gnu"),
        "manifest embedding currently targets the MSVC toolchain"
    );
}
