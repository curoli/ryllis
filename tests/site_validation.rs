use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn write_source(root: &Path, path: &str, markdown: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, markdown).unwrap();
}

fn ryllis(source: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ryllis"))
        .arg("--source")
        .arg(source)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn output_collisions_report_both_sources_before_export() {
    for nested in ["en/topic.html/index.md", "de/topic.html/index.md"] {
        let site = tempfile::tempdir().unwrap();
        let source = site.path().join("content");
        let output = site.path().join("output");
        write_source(&source, "en/index.md", "# Home");
        write_source(&source, "en/topic.md", "# Topic");
        write_source(&source, nested, "# Nested topic");

        for args in [
            vec!["check"],
            vec!["build", "--output", output.to_str().unwrap()],
        ] {
            let result = ryllis(&source, &args);
            let error = String::from_utf8(result.stderr).unwrap().replace('\\', "/");
            assert!(!result.status.success());
            assert!(error.contains("en/topic.md"), "{error}");
            assert!(error.contains(nested), "{error}");
            assert!(
                error.contains("both a generated file and a required directory"),
                "{error}"
            );
        }
        assert!(!output.exists());
    }
}

#[test]
fn check_uses_the_exported_spelling_of_language_prefixes() {
    let site = tempfile::tempdir().unwrap();
    let source = site.path();
    let valid_links = "[About](/en/about.html) [Default](/about.html) \
        [Home](/en/) [Index](/en/index.html) [Placeholder](/pt-BR/about.html)";
    write_source(source, "en/index.md", valid_links);
    write_source(source, "en/about.md", "# About");
    write_source(source, "pt-BR/index.md", "# Início");

    let result = ryllis(source, &["check"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    write_source(
        source,
        "en/index.md",
        &format!("{valid_links} [Wrong case](/EN/about.html) [Wrong case](/pt-br/index.html)"),
    );
    let result = ryllis(source, &["check"]);
    let error = String::from_utf8(result.stderr).unwrap();
    assert!(!result.status.success());
    assert!(error.contains("/en/ → /EN/about.html"), "{error}");
    assert!(error.contains("/en/ → /pt-br/index.html"), "{error}");
    assert!(error.contains("2 broken internal link(s)"), "{error}");
}
