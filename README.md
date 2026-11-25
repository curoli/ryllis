# Ryllis <img src="assets/ryllis-amaryllis.png" alt="" width="48" height="48">

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

## Configuration

On Unix-like systems, `build`, `check`, and `preview` look for `~/.config/ryllis/ryllis.toml`. If `XDG_CONFIG_HOME` is set to an absolute path, Ryllis looks for `ryllis/ryllis.toml` there. On other systems, Ryllis looks for `ryllis.toml` in the working directory.

Use `--config PATH` to select a project-specific file. The following example works in a config file saved at the repository root and selected with `--config ryllis.toml`.

```toml
source = "examples/site"
output = "target/example-site"
default_language = "en"
bind = "127.0.0.1:3000"
# template = "templates/page.html"
```

Relative paths in the file are resolved from the config file's directory. CLI paths and omitted directory defaults are resolved from the working directory. CLI flags override the corresponding settings.

The built-in defaults are `content` for the source, `target/ryllis-site` for the output, `en` for the default language, and `127.0.0.1:3000` for the preview address. Ryllis uses its built-in template unless you supply one.

### Static assets

Add an `assets` list to your site configuration to copy matching files into the build unchanged and serve them in preview.

```toml
assets = ["*.jpg", "*.svg", "*.css", "assets", "scripts/**/*.js"]
```

Patterns are case-sensitive globs. A pattern without a slash matches a file or folder name at any depth. A pattern with a slash matches a path relative to the source directory. Use `**/` to match any number of directory levels. When a pattern matches a folder, all files inside it are assets, including Markdown files. The list is empty by default.

Source-relative paths carry through to the output. For example, `assets/logo.svg` is shared at `/assets/logo.svg`, while `de/assets/banner.svg` is available at `/de/assets/banner.svg`. Top-level asset-only folders do not count as languages. Use root-relative URLs for shared assets and language-prefixed or page-relative URLs for localized assets. The preview also resolves unprefixed localized assets in the default language. The `check` command resolves Markdown links and images against exported pages and assets.

## How Ryllis works

1. Ryllis discovers the Markdown files in each language directory and builds an index of their paths and URLs.
2. Ryllis evaluates MiniJinja expressions, blocks, and comments in each Markdown file. A file with no template syntax passes through as written. Both kinds of files use the same `.md` extension.
3. Ryllis renders the resulting Markdown as HTML, collects its internal links for `check`, and wraps it in the shared HTML layout.
4. Ryllis creates a page for each language. Matching paths are translations of the same page, and missing translations get a placeholder. The build exports the language-prefixed directory tree, and the preview also serves unprefixed URLs in the default language.

### Templates in Markdown

Templates have access to `language`, the current language tag, and `pages`, the index of source files in that language. Each entry has a language-relative Markdown `path`, a `directory` (empty for files at the language root), and a language-prefixed `url`. A list based on this index includes pages written in the current language.

The example site's news index generates its links from `pages`. Use a MiniJinja raw block to include literal template syntax, even inside a fenced Markdown code block.

````markdown
{% raw %}
```jinja
{{ language }}
```
{% endraw %}
````

Template errors identify the Markdown file and line. The shared HTML layout is applied separately and uses the slots described above.
