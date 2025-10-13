#[test]
fn require_nextest_runner() {
    let using_nextest = std::env::var("NEXTEST")
        .map(|v| !v.is_empty())
        .unwrap_or(false);

    if !using_nextest {
        eprintln!(
            "\nLoTaR's test suite is powered by cargo-nextest. \n\n  • Local: run `cargo nextest run --all-features` or `npm run test`.\n  • Docs: see docs/ui-dev-setup.md for details.\n"
        );
        panic!(
            "The legacy `cargo test` runner is disabled. Please use `cargo nextest run --all-features` or `npm run test`."
        );
    }
}
