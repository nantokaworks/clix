fn main() {
    let date = time::OffsetDateTime::now_utc();
    println!(
        "cargo::rustc-env=FLYX_BUILD_DATE={:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    );
}
