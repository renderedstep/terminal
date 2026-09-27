fn main() {
    // The target triple picks the release asset `ta upgrade` downloads.
    println!(
        "cargo:rustc-env=TA_TARGET={}",
        std::env::var("TARGET").expect("cargo sets TARGET")
    );
}
