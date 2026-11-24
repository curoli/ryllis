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
        .env("XDG_CONFIG_HOME", source.join(".test-config"))
        .arg("--source")
        .arg(source)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn markdown_templates_generate_checked_links_and_preserve_raw_code() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("content");
    let output = temp.path().join("site");
    let page = "# News in {{ language }}\n\n{% raw %}`{{ example }}`{% endraw %}\n\n\
        {% for page in pages if page.path == 'post.md' %}[Post]({{ page.url }}){% endfor %}";
    write_source(&source, "en/index.md", page);
    write_source(&source, "en/post.md", "# Ordinary Markdown");
    let result = ryllis(&source, &["build", "--output", output.to_str().unwrap()]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let html = fs::read_to_string(output.join("en/index.html")).unwrap();
    assert!(html.contains("<h1>News in en</h1>"));
    assert!(html.contains("<code>{{ example }}</code>"));
    assert!(html.contains("<a href=\"/en/post.html\">Post</a>"));
    assert!(ryllis(&source, &["check"]).status.success());

    write_source(
        &source,
        "en/index.md",
        "# News\n[Broken]({{ '/missing.html' }})",
    );
    let result = ryllis(&source, &["check"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("/en/ → /missing.html"));

    write_source(&source, "en/index.md", "# News\n{% unknown %}");
    let result = ryllis(&source, &["check"]);
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(error.contains("index.md:2"), "{error}");
}

#[test]
fn project_config_paths_and_cli_overrides_use_their_own_directories() {
    let site = tempfile::tempdir().unwrap();
    let root = site.path();
    write_source(root, "project/articles/de/index.md", "# Configured source");
    write_source(root, "override/en/index.md", "# CLI source");
    fs::write(
        root.join("project/layout.html"),
        "<base href=\"{{path}}\"><article>{{content}}</article>",
    )
    .unwrap();
    fs::write(
        root.join("project/ryllis.toml"),
        r#"
source = "articles"
output = "public"
default_language = "de"
template = "layout.html"
bind = "127.0.0.1:4000"
"#,
    )
    .unwrap();

    let command = || Command::new(env!("CARGO_BIN_EXE_ryllis"));
    let check = command()
        .current_dir(root.join("project"))
        .args(["--config", "ryllis.toml", "check"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let build = command()
        .current_dir(root)
        .args(["--config", "project/ryllis.toml", "build"])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let page = fs::read_to_string(root.join("project/public/de/index.html")).unwrap();
    assert!(page.contains("<article><h1>Configured source</h1>"));

    let build = command()
        .current_dir(root)
        .args([
            "build",
            "--config",
            "project/ryllis.toml",
            "--source",
            "override",
            "--output",
            "export",
            "--default-language",
            "en",
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let page = fs::read_to_string(root.join("export/en/index.html")).unwrap();
    assert!(page.contains("<h1>CLI source</h1>"));
}

#[test]
fn invalid_and_explicitly_missing_config_files_are_reported() {
    let site = tempfile::tempdir().unwrap();
    let root = site.path();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ryllis"));
        command.current_dir(root).arg("check");
        command
    };

    let check = command()
        .args(["--config", "missing.toml"])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&check.stderr);
    assert!(!check.status.success());
    assert!(error.contains("reading config missing.toml"), "{error}");

    fs::write(root.join("ryllis.toml"), "soruce = 'content'").unwrap();
    let check = command()
        .args(["--config", "ryllis.toml"])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&check.stderr);
    assert!(!check.status.success());
    assert!(error.contains("parsing config ryllis.toml"), "{error}");
    assert!(error.contains("soruce"), "{error}");
}

#[cfg(unix)]
#[test]
fn unix_config_uses_home_or_xdg_and_defaults_to_working_directory_paths() {
    let site = tempfile::tempdir().unwrap();
    let root = site.path();
    write_source(
        root,
        "home/.config/ryllis/ryllis.toml",
        "default_language = 'de'",
    );
    write_source(root, "site/content/de/index.md", "# Home configuration");
    write_source(root, "xdg/ryllis/ryllis.toml", "source = 'articles'");
    write_source(
        root,
        "xdg/ryllis/articles/en/index.md",
        "# XDG configuration",
    );

    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ryllis"));
        command
            .current_dir(root.join("site"))
            .env("HOME", root.join("home"))
            .env_remove("XDG_CONFIG_HOME");
        command
    };
    let build = command()
        .args(["build", "--output", "home-output"])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(root.join("site/home-output/de/index.html").is_file());

    let build = command()
        .env("XDG_CONFIG_HOME", root.join("xdg"))
        .arg("build")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let page = fs::read_to_string(root.join("site/target/ryllis-site/en/index.html")).unwrap();
    assert!(page.contains("XDG configuration"));
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

#[test]
fn check_lists_present_and_missing_languages_for_each_incomplete_page() {
    let site = tempfile::tempdir().unwrap();
    let source = site.path();
    let home = "[German news](/de/news/)";
    write_source(source, "en/index.md", home);
    write_source(source, "de/index.md", "# Startseite");
    write_source(source, "es/index.md", "# Inicio");
    write_source(source, "fr/index.md", "# Accueil");
    write_source(source, "en/news/index.md", "# News");
    write_source(source, "fr/news/index.md", "# Actualités");
    write_source(source, "de/about.md", "# Über uns");
    write_source(source, "es/about.md", "# Sobre nosotros");

    let result = ryllis(source, &["check"]);
    let report = String::from_utf8(result.stderr).unwrap().replace('\\', "/");
    assert!(result.status.success(), "{report}");
    assert_eq!(
        report.lines().collect::<Vec<_>>(),
        [
            "about.md is present in de, es and missing in en, fr",
            "news/index.md is present in en, fr and missing in de, es",
        ],
    );

    write_source(
        source,
        "en/index.md",
        &format!("{home}\n\n[Broken](/missing.html)"),
    );
    let result = ryllis(source, &["check"]);
    let report = String::from_utf8(result.stderr).unwrap().replace('\\', "/");
    assert!(!result.status.success());
    assert!(
        report.contains("about.md is present in de, es and missing in en, fr"),
        "{report}"
    );
    assert!(report.contains("1 broken internal link(s)"), "{report}");

    write_source(source, "en/index.md", home);
    write_source(source, "de/news/index.md", "# Neuigkeiten");
    write_source(source, "es/news/index.md", "# Noticias");
    write_source(source, "en/about.md", "# About us");
    write_source(source, "fr/about.md", "# À propos");
    let result = ryllis(source, &["check"]);
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
}
