use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
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
    missing_translations: Vec<MissingTranslation>,
}

impl Site {
    pub fn load(source: &Path, default_language: &str, template: Option<&Path>) -> Result<Self> {
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
        for entry in
            fs::read_dir(source).with_context(|| format!("reading {}", source.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
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
            let mut pages = BTreeMap::new();
            read_content(&entry.path(), &entry.path(), &mut pages)?;
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
        let mut pages = BTreeMap::new();
        let mut missing_translations = Vec::new();
        for (output, (path, _)) in &output_files {
            let mut present = Vec::new();
            let mut missing = Vec::new();
            for (language, translations) in &content {
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
        Ok(Self {
            default_language,
            languages,
            pages,
            missing_translations,
        })
    }

    pub fn page(&self, path: &str) -> Option<&str> {
        let path = normalize_url_path(path)?;
        let path = match self.language_prefix(&path) {
            Some((language, rest)) => format!("/{language}{rest}"),
            None => format!("/{}{path}", self.default_language),
        };
        self.prefixed_page(&path)
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
        if self.language_prefix(&path).is_some() {
            self.prefixed_page(&path).is_some()
        } else {
            self.prefixed_page(&format!("/{}{path}", self.default_language))
                .is_some()
        }
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
        fs::rename(tree, output).with_context(|| format!("publishing {}", output.display()))?;
        Ok(self.pages.len())
    }
}

fn read_content(
    root: &Path,
    directory: &Path,
    pages: &mut BTreeMap<PathBuf, Markdown>,
) -> Result<()> {
    for entry in
        fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            read_content(root, &path, pages)?;
        } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "md") {
            let relative = path.strip_prefix(root)?.to_owned();
            ensure!(
                relative.to_str().is_some(),
                "Markdown path is not UTF-8: {}",
                path.display()
            );
            pages.insert(relative, render_markdown(&path, &read_text(&path)?));
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
            Event::Start(Tag::Link { dest_url, .. }) => links.push(dest_url.to_string()),
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
    let mut url = String::new();
    let directory_index = file.file_name().is_some_and(|name| name == "index.html");
    let path = if directory_index {
        file.parent().expect("index has a parent")
    } else {
        file
    };
    for part in path {
        url.push('/');
        url.push_str(&encode(part.to_str().expect("validated UTF-8 path")));
    }
    if directory_index {
        url.push('/');
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
