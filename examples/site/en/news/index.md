# Garden news

{% for page in pages if page.directory == "news" and page.path != "news/index.md" -%}
- [{{ page.path | replace("news/", "") | replace(".md", "") | replace("-", " ") | title }}]({{ page.url }})
{% endfor %}

[Back to the garden home page](../).
