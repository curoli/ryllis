use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use glob::{MatchOptions, Pattern};
use minijinja::{Environment, UndefinedBehavior, Value, context, syntax::SyntaxConfig};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use pulldown_cmark::{Event, Parser, Tag, html};
use url::Url;

const DEFAULT_TEMPLATE: &str = include_str!("page.html");
const URL_SEGMENT: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

struct Page {
    file: PathBuf,
    html: String,
    links: Vec<String>,
}

struct Asset {
    file: PathBuf,
    source: PathBuf,
}

struct Markdown {
    title: String,
    html: String,
    links: Vec<String>,
}

pub struct MissingTranslation {
    pub path: PathBuf,
    pub present: Vec<String>,
    pub missing: Vec<String>,
}

pub struct Site {
    default_language: String,
    languages: BTreeSet<String>,
    pages: BTreeMap<String, Page>,
    assets: BTreeMap<String, Asset>,
    missing_translations: Vec<MissingTranslation>,
}

impl Site {
    pub fn load(
        source: &Path,
        default_language: &str,
        template: Option<&Path>,
        assets: &[Pattern],
    ) -> Result<Self> {
        let template = match template {
            Some(path) => read_text(path)?,
            None => DEFAULT_TEMPLATE.to_owned(),
        };
        ensure!(
            template.contains("{{content}}") && template.contains("{{path}}"),
            "the template needs {} and {} slots",
            "{{content}}",
            "{{path}}"
        );

        let mut content = BTreeMap::new();
        let mut asset_files = BTreeMap::new();
        for entry in
            fs::read_dir(source).with_context(|| format!("reading {}", source.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            let matched = matches_asset(Path::new(&entry.file_name()), assets);
            if !kind.is_dir() {
                if kind.is_file() && matched {
                    add_asset(source, path, &mut asset_files)?;
                }
                continue;
            }
            let mut pages = BTreeMap::new();
            let asset_count = asset_files.len();
            read_content(
                source,
                &path,
                &path,
                assets,
                matched,
                &mut pages,
                &mut asset_files,
            )?;
            if matched || (pages.is_empty() && asset_count != asset_files.len()) {
                continue;
            }
            let language = entry.file_name().into_string().map_err(|name| {
                anyhow::anyhow!("language directory name is not UTF-8: {name:?}")
            })?;
            ensure!(
                valid_language(&language),
                "invalid language tag: {language}"
            );
            ensure!(
                !content
                    .keys()
                    .any(|tag: &String| tag.eq_ignore_ascii_case(&language)),
                "duplicate language tag: {language}"
            );
            content.insert(language, pages);
        }
        let default_language = content
            .keys()
            .find(|tag| tag.eq_ignore_ascii_case(default_language))
            .with_context(|| {
                format!("missing source directory for default language {default_language}")
            })?
            .clone();
        ensure!(
            content
                .values()
                .any(|pages| pages.contains_key(Path::new("index.md"))),
            "add an index.md to at least one language directory"
        );

        // Every language exports this full set, including missing-translation pages.
        let mut output_files = BTreeMap::new();
        for (language, translations) in &content {
            for path in translations.keys() {
                let output = path.with_extension("html");
                let (previous, previous_language) = *output_files
                    .entry(output.clone())
                    .or_insert((path, language));
                ensure!(
                    previous == path,
                    "{} and {} both generate {}",
                    Path::new(previous_language).join(previous).display(),
                    Path::new(language).join(path).display(),
                    output.display()
                );
            }
        }
        for (output, (source, language)) in &output_files {
            for directory in output.ancestors().skip(1) {
                if let Some((previous, previous_language)) = output_files.get(directory) {
                    anyhow::bail!(
                        "{} and {} conflict because {} is both a generated file and a required directory",
                        Path::new(previous_language).join(previous).display(),
                        Path::new(language).join(source).display(),
                        directory.display()
                    );
                }
            }
        }
        let languages = content.keys().cloned().collect();
        let mut environment = Environment::new();
        environment.set_undefined_behavior(UndefinedBehavior::Strict);
        environment.set_syntax(
            SyntaxConfig::builder()
                .keep_trailing_newline(true)
                .build()?,
        );
        let mut rendered = BTreeMap::new();
        for (language, sources) in &content {
            let pages = Value::from_iter(sources.keys().map(|path| {
                let url = page_url(&Path::new(language).join(path.with_extension("html")));
                let path = path
                    .to_str()
                    .expect("validated UTF-8 path")
                    .replace(std::path::MAIN_SEPARATOR, "/");
                let (directory, _) = path.rsplit_once('/').unwrap_or(("", &path));
                context! { path => &path, directory => directory, url => url }
            }));
            let template_context = context! { language => language, pages => pages };
            let mut translations = BTreeMap::new();
            for (relative, path) in sources {
                let source = read_text(path)?;
                let markdown = if ["{{", "{%", "{#"]
                    .iter()
                    .any(|marker| source.contains(marker))
                {
                    environment.render_named_str(
                        &path.to_string_lossy(),
                        &source,
                        template_context.clone(),
                    )?
                } else {
                    source
                };
                translations.insert(relative.clone(), render_markdown(path, &markdown));
            }
            rendered.insert(language, translations);
        }
        let mut pages = BTreeMap::new();
        let mut missing_translations = Vec::new();
        for (output, (path, _)) in &output_files {
            let mut present = Vec::new();
            let mut missing = Vec::new();
            for (&language, translations) in &rendered {
                let file = Path::new(language).join(output);
                let url = page_url(&file);
                let navigation = navigation(language, path, &languages);
                let (title, body, links) = match translations.get(*path) {
                    Some(markdown) => {
                        present.push(language);
                        (
                            markdown.title.as_str(),
                            markdown.html.clone(),
                            markdown.links.clone(),
                        )
                    }
                    None => {
                        missing.push(language);
                        (
                            "Translation unavailable",
                            missing_translation(language, translations),
                            Vec::new(),
                        )
                    }
                };
                let html = render_template(
                    &template,
                    &[
                        ("language", language),
                        ("title", &escape(title)),
                        ("path", &escape(&url)),
                        ("navigation", &navigation),
                        ("content", &body),
                    ],
                );
                pages.insert(url, Page { file, html, links });
            }
            if !missing.is_empty() {
                missing_translations.push(MissingTranslation {
                    path: (*path).clone(),
                    present: present.into_iter().cloned().collect(),
                    missing: missing.into_iter().cloned().collect(),
                });
            }
        }
        let mut exported_files = asset_files
            .values()
            .map(|asset| asset.file.as_path())
            .collect::<BTreeSet<_>>();
        for page in pages.values() {
            ensure!(
                exported_files.insert(&page.file),
                "asset conflicts with generated page {}",
                page.file.display()
            );
        }
        for file in &exported_files {
            for directory in file.ancestors().skip(1) {
                ensure!(
                    !exported_files.contains(directory),
                    "cannot export {} because {} is both a file and a required directory",
                    file.display(),
                    directory.display()
                );
            }
        }
        Ok(Self {
            default_language,
            languages,
            pages,
            assets: asset_files,
            missing_translations,
        })
    }

    pub fn preview(&self, path: &str) -> Result<Option<(String, Vec<u8>)>> {
        let Some(path) = normalize_url_path(path) else {
            return Ok(None);
        };
        let fallback = match self.language_prefix(&path) {
            Some((language, rest)) => format!("/{language}{rest}"),
            None => format!("/{}{path}", self.default_language),
        };
        if let Some(asset) = self
            .assets
            .get(&path)
            .or_else(|| self.assets.get(&fallback))
        {
            let bytes = fs::read(&asset.source)
                .with_context(|| format!("reading {}", asset.source.display()))?;
            let mime = mime_guess::from_path(&asset.file).first_or_octet_stream();
            return Ok(Some((mime.to_string(), bytes)));
        }
        Ok(self
            .prefixed_page(&fallback)
            .map(|html| ("text/html; charset=utf-8".into(), html.as_bytes().to_vec())))
    }

    fn language_prefix<'a>(&self, path: &'a str) -> Option<(&str, &'a str)> {
        let path = path.strip_prefix('/')?;
        let end = path.find('/').unwrap_or(path.len());
        let (prefix, rest) = path.split_at(end);
        let language = self
            .languages
            .iter()
            .find(|tag| tag.eq_ignore_ascii_case(prefix))?;
        Some((language, rest))
    }

    fn prefixed_page(&self, path: &str) -> Option<&str> {
        let canonical = path
            .strip_suffix("/index.html")
            .map(|base| format!("{base}/"));
        let directory = format!("{path}/");
        self.pages
            .get(path)
            .or_else(|| canonical.as_ref().and_then(|url| self.pages.get(url)))
            .or_else(|| self.pages.get(&directory))
            .map(|page| page.html.as_str())
    }

    fn exported_link_exists(&self, path: &str) -> bool {
        let Some(path) = normalize_url_path(path) else {
            return false;
        };
        let page_path = if self.language_prefix(&path).is_some() {
            path.clone()
        } else {
            format!("/{}{path}", self.default_language)
        };
        self.assets.contains_key(&path)
            || self.assets.contains_key(&page_path)
            || self.prefixed_page(&page_path).is_some()
    }

    pub fn missing_translations(&self) -> &[MissingTranslation] {
        &self.missing_translations
    }

    pub fn broken_links(&self) -> BTreeSet<String> {
        let origin = Url::parse("https://ryllis.invalid/").expect("valid static origin");
        let mut broken = BTreeSet::new();
        for (path, page) in &self.pages {
            let base = origin.join(path).expect("valid generated path");
            for link in &page.links {
                // Absolute and protocol-relative URLs refer to the deployed site or other sites.
                if Url::parse(link).is_ok() || link.starts_with("//") {
                    continue;
                }
                match base.join(link) {
                    Ok(target) if target.origin() != origin.origin() => {}
                    Ok(target) if self.exported_link_exists(target.path()) => {}
                    _ => {
                        broken.insert(format!("{path} → {link}"));
                    }
                }
            }
        }
        broken
    }

    pub fn write(&self, output: &Path) -> Result<usize> {
        ensure!(
            !output.try_exists()?,
            "{} already exists. Choose a new output directory with --output.",
            output.display()
        );
        let parent = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let staged = tempfile::Builder::new()
            .prefix(".ryllis-")
            .tempdir_in(parent)?;
        let tree = staged.path().join("site");
        fs::create_dir(&tree)?;
        for page in self.pages.values() {
            let file = tree.join(&page.file);
            fs::create_dir_all(file.parent().expect("generated file has a parent"))?;
            let mut destination = fs::File::create_new(&file)
                .with_context(|| format!("creating generated file {}", page.file.display()))?;
            destination
                .write_all(page.html.as_bytes())
                .with_context(|| format!("writing {}", page.file.display()))?;
        }
        for asset in self.assets.values() {
            let destination = tree.join(&asset.file);
            fs::create_dir_all(destination.parent().expect("asset file has a parent"))?;
            (|| -> io::Result<()> {
                let mut source = fs::File::open(&asset.source)?;
                let mut destination = fs::File::create_new(destination)?;
                io::copy(&mut source, &mut destination)?;
                Ok(())
            })()
            .with_context(|| format!("copying asset {}", asset.source.display()))?;
        }
        fs::rename(tree, output).with_context(|| format!("publishing {}", output.display()))?;
        Ok(self.pages.len())
    }
}

fn matches_asset(relative: &Path, patterns: &[Pattern]) -> bool {
    let Some(path) = relative.to_str() else {
        return false;
    };
    let path = path.replace(std::path::MAIN_SEPARATOR, "/");
    let name = path.rsplit('/').next().expect("path has a file name");
    let options = MatchOptions {
        require_literal_separator: true,
        ..MatchOptions::new()
    };
    patterns.iter().any(|pattern| {
        let candidate = if pattern.as_str().contains('/') {
            path.as_str()
        } else {
            name
        };
        pattern.matches_with(candidate, options)
    })
}

fn add_asset(root: &Path, source: PathBuf, assets: &mut BTreeMap<String, Asset>) -> Result<()> {
    let file = source.strip_prefix(root)?.to_owned();
    ensure!(
        file.to_str().is_some(),
        "asset path is not UTF-8: {}",
        source.display()
    );
    assets.insert(file_url(&file), Asset { file, source });
    Ok(())
}

fn read_content(
    root: &Path,
    language_root: &Path,
    directory: &Path,
    patterns: &[Pattern],
    asset_directory: bool,
    pages: &mut BTreeMap<PathBuf, PathBuf>,
    assets: &mut BTreeMap<String, Asset>,
) -> Result<()> {
    for entry in
        fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        let matched = asset_directory || matches_asset(path.strip_prefix(root)?, patterns);
        if kind.is_dir() {
            read_content(root, language_root, &path, patterns, matched, pages, assets)?;
        } else if kind.is_file() && matched {
            add_asset(root, path, assets)?;
        } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "md") {
            let relative = path.strip_prefix(language_root)?.to_owned();
            ensure!(
                relative.to_str().is_some(),
                "Markdown path is not UTF-8: {}",
                path.display()
            );
            pages.insert(relative, path);
        }
    }
    Ok(())
}

fn render_markdown(path: &Path, source: &str) -> Markdown {
    let parser = Parser::new(source);
    let mut title = String::new();
    let mut reading_heading = false;
    let mut found_heading = false;
    let mut links = Vec::new();
    let events = parser.map(|event| {
        match &event {
            Event::Start(Tag::Heading { .. }) if !found_heading => reading_heading = true,
            Event::End(pulldown_cmark::TagEnd::Heading(_)) if reading_heading => {
                reading_heading = false;
                found_heading = true;
            }
            Event::Text(text) | Event::Code(text) if reading_heading => title.push_str(text),
            Event::SoftBreak | Event::HardBreak if reading_heading => title.push(' '),
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                links.push(dest_url.to_string())
            }
            _ => {}
        }
        match event {
            Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
            other => other,
        }
    });
    let mut html = String::new();
    html::push_html(&mut html, events);
    if title.trim().is_empty() {
        title = path
            .file_stem()
            .expect("Markdown file has a stem")
            .to_string_lossy()
            .into_owned();
    }
    Markdown { title, html, links }
}

fn navigation(language: &str, path: &Path, languages: &BTreeSet<String>) -> String {
    let mut nav = format!("<a href=\"/{language}/\">Home</a>");
    for tag in languages {
        let url = page_url(&Path::new(tag).join(path.with_extension("html")));
        let current = if tag == language {
            " aria-current=\"page\""
        } else {
            ""
        };
        write!(
            nav,
            " <a href=\"{url}\" lang=\"{tag}\" hreflang=\"{tag}\"{current}>{tag}</a>"
        )
        .expect("writing to a string");
    }
    nav
}

fn missing_translation(language: &str, translations: &BTreeMap<PathBuf, Markdown>) -> String {
    let mut body = format!(
        "<section lang=\"en\"><h1>Translation unavailable</h1><p>This page is unavailable in {language}.</p>"
    );
    if translations.is_empty() {
        body.push_str("<p>No pages are available in this language yet.</p>");
    } else {
        body.push_str("<h2>Pages available in this language</h2><ul>");
        for (path, page) in translations {
            let url = page_url(&Path::new(language).join(path.with_extension("html")));
            write!(
                body,
                "<li><a href=\"{url}\" lang=\"{language}\">{}</a></li>",
                escape(&page.title)
            )
            .expect("writing to a string");
        }
        body.push_str("</ul>");
    }
    body.push_str("</section>");
    body
}

fn render_template(template: &str, slots: &[(&str, &str)]) -> String {
    let mut pieces = template.split("{{");
    let mut html = pieces.next().unwrap_or_default().to_owned();
    for piece in pieces {
        if let Some((name, rest)) = piece.split_once("}}")
            && let Some((_, value)) = slots.iter().find(|(slot, _)| *slot == name)
        {
            html.push_str(value);
            html.push_str(rest);
        } else {
            html.push_str("{{");
            html.push_str(piece);
        }
    }
    html
}

fn valid_language(language: &str) -> bool {
    let mut parts = language.split('-');
    let primary = parts.next().unwrap_or_default();
    (2..=8).contains(&primary.len())
        && primary.bytes().all(|byte| byte.is_ascii_alphabetic())
        && parts.all(|part| {
            (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

fn page_url(file: &Path) -> String {
    if file.file_name().is_some_and(|name| name == "index.html") {
        format!("{}/", file_url(file.parent().expect("index has a parent")))
    } else {
        file_url(file)
    }
}

fn file_url(path: &Path) -> String {
    let mut url = String::new();
    for part in path {
        url.push('/');
        url.push_str(&encode(part.to_str().expect("validated UTF-8 path")));
    }
    url
}

fn read_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn normalize_url_path(path: &str) -> Option<String> {
    path.split('/')
        .map(|segment| {
            percent_decode_str(segment)
                .decode_utf8()
                .ok()
                .map(|text| encode(&text))
        })
        .collect::<Option<Vec<_>>>()
        .map(|segments| segments.join("/"))
}

fn encode(segment: &str) -> String {
    utf8_percent_encode(segment, URL_SEGMENT).to_string()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_serves_shared_and_localized_assets() {
        let source = tempfile::tempdir().unwrap();
        let root = source.path();
        fs::create_dir(root.join("en")).unwrap();
        fs::create_dir(root.join("de")).unwrap();
        fs::write(root.join("en/index.md"), "# Home").unwrap();
        fs::write(root.join("de/index.md"), "# Startseite").unwrap();
        fs::write(root.join("site.css"), "body {}").unwrap();
        fs::write(root.join("en/banner image.svg"), "<svg/>").unwrap();

        let patterns = ["*.css", "*.svg"].map(|pattern| Pattern::new(pattern).unwrap());
        let site = Site::load(root, "en", None, &patterns).unwrap();
        assert_eq!(
            site.preview("/site.css").unwrap().unwrap(),
            ("text/css".into(), b"body {}".to_vec())
        );
        for url in ["/en/banner%20image.svg", "/banner%20image.svg"] {
            assert_eq!(
                site.preview(url).unwrap().unwrap(),
                ("image/svg+xml".into(), b"<svg/>".to_vec())
            );
        }
        assert!(site.preview("/de/banner%20image.svg").unwrap().is_none());
        assert!(site.preview("/en/index.md").unwrap().is_none());
    }

    #[test]
    fn export_does_not_publish_when_two_pages_resolve_to_the_same_file() {
        let destination = tempfile::tempdir().unwrap();
        let output = destination.path().join("site");
        // Use the same file path to reproduce the collision on case-sensitive filesystems.
        let pages = ["/en/About.html", "/en/about.html"]
            .into_iter()
            .map(|url| {
                let page = Page {
                    file: Path::new("en").join("About.html"),
                    html: url.to_owned(),
                    links: Vec::new(),
                };
                (url.to_owned(), page)
            })
            .collect();
        let site = Site {
            default_language: "en".to_owned(),
            languages: BTreeSet::from(["en".to_owned()]),
            pages,
            assets: BTreeMap::new(),
            missing_translations: Vec::new(),
        };

        let error = site.write(&output).unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::AlreadyExists,
        );
        assert!(!output.exists());
        assert!(destination.path().read_dir().unwrap().next().is_none());
    }
}
