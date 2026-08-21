#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(not(debug_assertions), not(feature = "custom-protocol")))]
compile_error!(
    "KnightFrame release executables must enable custom-protocol; use `pnpm build:test-exe`"
);

fn main() {
    knightframe_lib::run();
}
