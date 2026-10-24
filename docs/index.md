{.janus-landing}
# Janus

```{raw} html
<section class="janus-hero">
  <svg class="janus-hero-mark" viewBox="-61 -83 122 166" aria-hidden="true" focusable="false">
    <g fill="none" stroke-width="18" stroke-linecap="round">
      <path class="janus-mark-top" d="M 0,2 L 0,-46 Q 0,-74 30,-74 Q 52,-74 52,-52"></path>
      <path class="janus-mark-bottom" d="M 0,-2 L 0,46 Q 0,74 -30,74 Q -52,74 -52,52"></path>
    </g>
  </svg>
  <p class="janus-hero-title"><span class="janus-ja">Your character</span> <span class="janus-nus">remembers.</span></p>
  <p class="janus-hero-tagline">Every detail, every session, forever &mdash; and it&rsquo;s yours, not a subscription.</p>
  <p class="janus-hero-lead">Janus is a private roleplay chat app for your desktop. Your characters, chats and settings stay on your computer, and you choose which AI model writes the replies.</p>
</section>
```

:::{div} janus-hero-actions
```{button-ref} user/getting-started
:ref-type: doc
:color: primary
:class: janus-btn
Get started
```

```{button-link} https://github.com/RamishSiddiqui/janus/releases/latest
:color: warning
:outline:
:class: janus-btn
Download
```

```{button-ref} developer/index
:ref-type: doc
:color: secondary
:outline:
:class: janus-btn
For developers
```
:::

## Why Janus

::::{grid} 1 1 2 2
:gutter: 3
:class-container: janus-features

:::{grid-item-card} Your story doesn't get amnesia
:link: user/how-janus-remembers
:link-type: doc
:class-card: janus-card

A layered memory (token budget, sliding window, rolling summary and vector search) keeps a conversation coherent for hundreds of messages instead of quietly cutting off the past.
:::

:::{grid-item-card} Private by construction
:link: user/data-and-privacy
:link-type: doc
:class-card: janus-card

Everything lives in a database on your own machine. Export and import give you a portable backup whenever you want one.
:::

:::{grid-item-card} Bring any model
:link: user/providers-and-models
:link-type: doc
:class-card: janus-card

Run one on your own computer with Ollama, or connect OpenRouter, Anthropic, Gemini, Groq and more. There are free ways to start.
:::

:::{grid-item-card} Real characters, not scripts
:link: user/characters
:link-type: doc
:class-card: janus-card

Import standard character cards (PNG or JSON), write your own, play as a persona, and add lorebooks. Group scenes track new speakers for you.
:::

:::{grid-item-card} Hear them, see them
:link: user/voice
:link-type: doc
:class-card: janus-card

A built-in voice that runs offline (Windows for now) or your own cloud voice key, plus scene art from AI Horde, ComfyUI or any compatible image service.
:::

:::{grid-item-card} On your phone too
:link: user/browser-access
:link-type: doc
:class-card: janus-card

Open Janus from a browser or phone on your home network, with accounts for the people you share it with.
:::
::::

## Start in three steps

::::{grid} 1 1 3 3
:gutter: 3
:class-container: janus-steps

:::{grid-item-card} Install
:link: https://github.com/RamishSiddiqui/janus/releases/latest
:link-type: url
:class-card: janus-card janus-step

Download the installer for Windows, macOS or Linux from the Releases page. After that, updates arrive from inside the app.
:::

:::{grid-item-card} Connect a model
:link: user/providers-and-models
:link-type: doc
:class-card: janus-card janus-step

The setup guide walks you through it: run a model on your computer, or paste a key for a cloud provider.
:::

:::{grid-item-card} Start a chat
:link: user/chatting
:link-type: doc
:class-card: janus-card janus-step

Pick a starter character, import a card, or create your own, and say hello.
:::
::::

## Go deeper

::::{grid} 1 1 3 3
:gutter: 3

:::{grid-item-card} User guide
:link: user/index
:link-type: doc
:class-card: janus-card

Every feature: chat, memory, lorebooks, scenes, voice, accounts and browser access.
:::

:::{grid-item-card} Developer guide
:link: developer/index
:link-type: doc
:class-card: janus-card

Architecture, the backend and frontend, the context engine, providers, testing and releases.
:::

:::{grid-item-card} Reference
:link: reference/commands
:link-type: doc
:class-card: janus-card

Generated from the source: every command, event, setting, database table and HTTP endpoint.
:::
::::

:::{div} janus-footnote
Janus is open source under the AGPL-3.0-or-later licence. [Source on GitHub](https://github.com/RamishSiddiqui/janus).
:::

```{toctree}
:hidden:
:caption: User guide

user/index
```

```{toctree}
:hidden:
:caption: Developer guide

developer/index
```

```{toctree}
:hidden:
:caption: Reference

reference/commands
reference/events
reference/settings
reference/database
reference/http-api
reference/source-map
```
