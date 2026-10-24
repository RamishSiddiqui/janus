"""Sphinx configuration for the Janus documentation."""

import os

project = "Janus"
author = "Janus contributors"
copyright = "2026, Janus contributors (AGPL-3.0-or-later)"

extensions = [
    "myst_parser",
    "sphinx_copybutton",
    "sphinx_design",
]

source_suffix = {".md": "markdown", ".rst": "restructuredtext"}
root_doc = "index"
exclude_patterns = ["_build", "requirements.txt", "Thumbs.db", ".DS_Store", "tools"]

myst_heading_anchors = 3
myst_enable_extensions = ["colon_fence", "attrs_block"]

html_theme = "furo"
html_title = "Janus"
# On Read the Docs this is the canonical address of the project's default
# version, so search engines index that one instead of every release's copy.
# Anywhere else (local builds, CI) it stays empty and adds nothing.
html_baseurl = os.environ.get("READTHEDOCS_CANONICAL_URL", "")
html_static_path = ["_static"]
templates_path = ["_templates"]  # sidebar/brand.html: the mark and wordmark
html_css_files = ["janus.css"]  # self-hosted brand fonts and the sidebar brand
html_js_files = [("janus-headings.js", {"defer": "defer"})]  # two-tone page titles
html_favicon = "_static/favicon.svg"

# The app's type: Raleway for text and headings, Geist Mono for code.
_SANS = 'Raleway, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif'
_MONO = '"Geist Mono", ui-monospace, "SF Mono", "Fira Code", Menlo, Consolas, monospace'

html_theme_options = {
    "source_repository": "https://github.com/RamishSiddiqui/janus/",
    "source_branch": "master",
    "source_directory": "docs/",
    # Brand colours from branding/janus-kit/BRAND.md: violet #9075F2, gold #CDA15F,
    # the light and dark page colours and text. Violet and gold are darkened a
    # step in light mode so links and the wordmark stay readable on the cream page.
    "light_css_variables": {
        "font-stack": _SANS,
        "font-stack--headings": _SANS,
        "font-stack--monospace": _MONO,
        "color-background-primary": "#F6F3EE",
        "color-background-secondary": "#EDE8DF",
        "color-foreground-primary": "#211F2E",
        "color-brand-primary": "#6d4fd6",
        "color-brand-content": "#6d4fd6",
        "janus-violet": "#6d4fd6",
        "janus-gold": "#8c6016",
        "janus-on-violet": "#ffffff",
    },
    "dark_css_variables": {
        "font-stack": _SANS,
        "font-stack--headings": _SANS,
        "font-stack--monospace": _MONO,
        "color-background-primary": "#0A0A13",
        "color-background-secondary": "#12121D",
        "color-foreground-primary": "#ECE9F6",
        "color-brand-primary": "#b9a7ff",
        "color-brand-content": "#b9a7ff",
        "janus-violet": "#9075F2",
        "janus-gold": "#CDA15F",
        "janus-on-violet": "#0A0A13",
    },
}
