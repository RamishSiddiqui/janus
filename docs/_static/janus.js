/*
  Two small touches that need a little script. Without JavaScript the docs
  still work: the title stays one colour and the top bar says "Janus".

  1. Two-tone page titles, the way the app draws them (SplitHeading.svelte): the
     first ~40% in violet, the rest in gold. Only the page title (the h1) is
     split; section headings stay solid so the title keeps meaning.

     The app snaps to the first space, which suits its short titles. Documentation
     titles are longer, so a title with several words splits at the word boundary
     nearest the 40% mark instead of cutting through a word. A single word splits
     by letters, exactly like the app ("Prov|iders").

  2. The Janus mark and wordmark in the top bar that replaces the sidebar on a
     narrow window. Furo hard-codes that bar's text in its page template with no
     block to override, so it is swapped here instead of copying the whole
     template. The sidebar's own brand block is a normal template
     (_templates/sidebar/brand.html).
*/
(function () {
  "use strict";

  function splitIndex(text) {
    var target = Math.round(text.length * 0.4);
    var best = -1;
    for (var i = text.indexOf(" "); i !== -1; i = text.indexOf(" ", i + 1)) {
      if (best === -1 || Math.abs(i - target) < Math.abs(best - target)) best = i;
    }
    if (best > 0) return best;
    return Math.max(1, target);
  }

  function apply(heading) {
    var node = heading.firstChild;
    // Plain-text titles only: a title that starts with code or a link is left alone.
    if (!node || node.nodeType !== 3) return;
    var text = node.nodeValue;
    if (!text || !text.trim()) return;

    var at = splitIndex(text);
    var first = document.createElement("span");
    first.className = "janus-split-a";
    first.textContent = text.slice(0, at);
    var second = document.createElement("span");
    second.className = "janus-split-b";
    second.textContent = text.slice(at);

    heading.replaceChild(second, node);
    heading.insertBefore(first, second);
  }

  var SVG_NS = "http://www.w3.org/2000/svg";

  // The same two strokes as JanusMark.svelte and the sidebar brand.
  function brandMark() {
    var svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("class", "janus-mark");
    svg.setAttribute("viewBox", "-61 -83 122 166");
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");
    var group = document.createElementNS(SVG_NS, "g");
    group.setAttribute("fill", "none");
    group.setAttribute("stroke-width", "18");
    group.setAttribute("stroke-linecap", "round");
    [
      ["janus-mark-top", "M 0,2 L 0,-46 Q 0,-74 30,-74 Q 52,-74 52,-52"],
      ["janus-mark-bottom", "M 0,-2 L 0,46 Q 0,74 -30,74 Q -52,74 -52,52"],
    ].forEach(function (stroke) {
      var path = document.createElementNS(SVG_NS, "path");
      path.setAttribute("class", stroke[0]);
      path.setAttribute("d", stroke[1]);
      group.appendChild(path);
    });
    svg.appendChild(group);
    return svg;
  }

  function span(className, text) {
    var el = document.createElement("span");
    el.className = className;
    el.textContent = text;
    return el;
  }

  function headerBrand() {
    var brand = document.querySelector(".mobile-header .brand");
    if (!brand) return;
    var name = brand.textContent.trim() || "Janus";

    var lockup = span("janus-brand janus-brand-header", "");
    lockup.setAttribute("aria-hidden", "true");
    lockup.appendChild(brandMark());
    var word = span("janus-wordmark", "");
    word.appendChild(span("janus-ja", "JA"));
    word.appendChild(span("janus-nus", "NUS"));
    lockup.appendChild(word);

    // Keep the real name for screen readers; the lockup is decoration.
    brand.textContent = "";
    brand.appendChild(span("janus-visually-hidden", name));
    brand.appendChild(lockup);
  }

  if (typeof module !== "undefined" && module.exports) {
    module.exports = { splitIndex: splitIndex };
    return;
  }

  headerBrand();

  var titles = document.querySelectorAll("article h1");
  for (var i = 0; i < titles.length; i++) {
    // The landing page hides its own h1; the hero headline is already two-tone.
    if (titles[i].parentElement && titles[i].parentElement.classList.contains("janus-landing")) continue;
    apply(titles[i]);
  }
})();
