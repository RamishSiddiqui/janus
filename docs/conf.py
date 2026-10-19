"""Sphinx configuration for the Janus documentation."""

project = "Janus"
author = "Janus contributors"
copyright = "2026, Janus contributors (AGPL-3.0-or-later)"

extensions = [
    "myst_parser",
    "sphinx_copybutton",
]

source_suffix = {".md": "markdown", ".rst": "restructuredtext"}
root_doc = "index"
exclude_patterns = ["_build", "requirements.txt", "Thumbs.db", ".DS_Store"]

myst_heading_anchors = 3

html_theme = "furo"
html_title = "Janus"
html_static_path = ["_static"]
html_theme_options = {
    "source_repository": "https://github.com/RamishSiddiqui/janus/",
    "source_branch": "master",
    "source_directory": "docs/",
    # Brand colours: violet and gold, matching the app.
    "light_css_variables": {
        "color-brand-primary": "#6d4fd6",
        "color-brand-content": "#6d4fd6",
    },
    "dark_css_variables": {
        "color-brand-primary": "#b9a7ff",
        "color-brand-content": "#b9a7ff",
    },
}
