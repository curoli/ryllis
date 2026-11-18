# ryllis

Website creator and maintainer making updates simple. We don't have anything yet, but we have plans.

## The plan

Ryllis is a Rust app to help set up and maintain a website.

Ryllis compiles source content into web pages.

The source content resides in the source directory and its sub-directories and consists of Markdown files with Ryllis extensions. In the simplest case, each Markdown file becomes an HTML file, and the structure of the source directory tree is preserved as the structure of the target HTML directory tree. In the simplest case, the source root directory and each sub-directory contains an `index.md` file that becomes an `index.html`.

The Markdown files define the core content. Ryllis renders the content as HTML and adds styling as well as navigation elements and other elements that are shared by the pages. For these added elements, Ryllis uses templates that are separate from the source content.

Ryllis supports multilingual websites. In this case, the content root directory contains a sub-directory for each language whose name is the corresponding language tag, such as "en" for English, and then Ryllis adds a selector to each page to switch language.

Upon enabling AI by setting a feature (e.g. "openai") and providing the appropriate API key, Ryllis can offer, whenever content is added or changed, to translate it to some or all of the other supported languages.

Ryllis should help as much as possible to set the site up and running. There should be feature flags that enable Ryllis to help configuring a chosen web service to serve the files (for example "nginx" or "caddy"). Initially, we will assume a Debian/Ubuntu-like system.

Ryllis also supports generating overviews over all files found in a given directory tree. This feature is invoked by the appropriate Markdown extension. Ryllis can be directed to understand certain meanings of folder names, for example, that "de" stands for German, or that"2026/09/26" stands for September 26, 2026. This feature can be used to generate an inventory of things offered, or a blog or newsfeed.

Ryllis can also test whether all pages are available at the expected URLs.

Every page is available through a path that starts with a language tag. When a path without language tag is requested, a language is chosen and the request is internally routed to the appropriate path. The language may be chosen by considering the requested domain, or the location of the requester. Otherwise, there is a default language. If no other default language is set, it is English.

Ryllis should be as helpful as possible. Ideally, the user runs Ryllis, follows directions provided by Ryllis, edits some content, and ends up with a decent running website.
