/*
  Two-tone page titles, the way the app draws them (SplitHeading.svelte): the
  first ~40% in violet, the rest in gold. Only the page title (the h1) is split;
  section headings stay solid so the title keeps meaning. Without JavaScript the
  title simply stays one colour.

  The app snaps to the first space, which suits its short titles. Documentation
  titles are longer, so a title with several words splits at the word boundary
  nearest the 40% mark instead of cutting through a word. A single word splits
  by letters, exactly like the app ("Prov|iders").
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

  if (typeof module !== "undefined" && module.exports) {
    module.exports = { splitIndex: splitIndex };
    return;
  }

  var titles = document.querySelectorAll("article h1");
  for (var i = 0; i < titles.length; i++) {
    // The landing page hides its own h1; the hero headline is already two-tone.
    if (titles[i].parentElement && titles[i].parentElement.classList.contains("janus-landing")) continue;
    apply(titles[i]);
  }
})();
