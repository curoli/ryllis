# ryllis

Website creator and maintainer making updates simple. The Rust prototype builds and previews a multilingual Markdown site.

## Try the prototype

```sh
cargo run -- --source examples/site check
cargo run -- --source examples/site build
cargo run -- --source examples/site preview
```

Open `http://127.0.0.1:3000/` for the English home page, `/de/` for the German home page, or `/de/news/` for a missing-translation page. Reload after editing a Markdown file to see the update. The preview serves unprefixed routes such as `/news/` internally in the default language. Use `--default-language de` to change it.

Put Markdown under language directories, even for a single-language site. Matching relative paths are translations of the same page. At least one language directory needs a homepage. An `index.md` is available at its directory URL and at `index.html`. Other Markdown files use the `.html` extension in their URLs. Use those URLs in Markdown links, for example `[About](about.html)`. Missing translations get generated pages that list the available pages in the requested language.

The build writes to `target/ryllis-site`. Use `--output PATH` to select another new directory, including for subsequent builds. The output contains the language-prefixed HTML tree. A static host needs a rewrite rule to serve unprefixed URLs in the default language. The `check` command validates internal Markdown links against generated pages, including placeholders. Match the case of the language directory when writing a language-prefixed link.

`check` reports each page with incomplete translations by its Markdown path relative to the language directories. It lists every language where the page is present and every language where it is missing. The example reports `news/index.md is present in en and missing in de`. Missing translations are warnings, and broken internal links cause the command to fail.

Use `--template FILE` to supply a shared HTML template. Include `{{content}}` for the rendered body and `<base href="{{path}}">` so relative links resolve under both prefixed and unprefixed URLs. The optional slots are `{{language}}`, `{{title}}`, and `{{navigation}}`.

## Project configuration

Create `ryllis.toml` in the directory where you run Ryllis. For the example site, you could use these settings.

```toml
source = "examples/site"
output = "target/example-site"
default_language = "en"
bind = "127.0.0.1:3000"
# template = "templates/page.html"
```

`build`, `check`, and `preview` read this file automatically. Use `--config PATH` to select a different file. Relative paths in the file, including omitted directory defaults, are resolved from the config file's directory. Paths supplied as CLI flags are resolved from the working directory. CLI flags override the corresponding settings.

The built-in defaults are `content` for the source, `target/ryllis-site` for the output, `en` for the default language, and `127.0.0.1:3000` for the preview address. Ryllis uses its built-in template unless you supply one.

## The plan

Ryllis is a Rust app to help set up and maintain a website.

Ryllis compiles source content into web pages.

The source content resides in the source directory and its sub-directories and consists of Markdown files with Ryllis extensions. Each Markdown file becomes an HTML file, and the structure of the source directory tree is preserved in the target HTML directory tree. An `index.md` in a language directory or any of its subdirectories becomes an `index.html`.

The Markdown files define the core content. Ryllis renders the content as HTML and adds styling as well as navigation elements and other elements that are shared by the pages. For these added elements, Ryllis uses templates that are separate from the source content.

The content root directory contains a subdirectory for each language, such as "en" for English. Ryllis adds a language selector to each page. A missing translation gets a placeholder that lists the pages available in the selected language.

Upon enabling AI by setting a feature (e.g. "openai") and providing the appropriate API key, Ryllis can offer, whenever content is added or changed, to translate it to some or all of the other supported languages.

Ryllis should help as much as possible to set the site up and running. There should be feature flags that enable Ryllis to help configuring a chosen web service to serve the files (for example "nginx" or "caddy"). Initially, we will assume a Debian/Ubuntu-like system.

Ryllis also supports generating overviews over all files found in a given directory tree. This feature is invoked by the appropriate Markdown extension. Ryllis can be directed to understand certain meanings of folder names, for example, that "de" stands for German, or that "2026/09/26" stands for September 26, 2026. This feature can be used to generate an inventory of things offered, or a blog or newsfeed.

Ryllis can also test whether all pages are available at the expected URLs.

Every page is available through a path that starts with a language tag. When a path without language tag is requested, a language is chosen and the request is internally routed to the appropriate path. The language may be chosen by considering the requested domain, or the location of the requester. Otherwise, there is a default language. If no other default language is set, it is English.

Ryllis should be as helpful as possible. Ideally, the user runs Ryllis, follows directions provided by Ryllis, edits some content, and ends up with a decent running website.
