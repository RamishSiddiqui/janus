<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    characterId = null,
    characterName,
    characterTagline,
    avatarUrl = null,
    tags = [],
    additionalCharacters = [],
  }: {
    characterId?: string | null;
    characterName: string;
    characterTagline: string;
    avatarUrl?: string | null;
    tags?: { label: string; color: string }[];
    additionalCharacters?: { id: string; name: string; description: string; avatarUrl: string | null; avatarColor: string }[];
  } = $props();

  // Carousel state for multi-character conversations
  let activeCardIndex = $state(0);
  interface CharCard { id: string | null; name: string; tagline: string; avatarUrl: string | null; avatarColor: string; tags: { label: string; color: string }[] }
  let allCards = $derived.by((): CharCard[] => {
    const primary: CharCard = { id: characterId, name: characterName, tagline: characterTagline, avatarUrl, avatarColor: '#8B5CF6', tags: tags || [] };
    if (!additionalCharacters || additionalCharacters.length === 0) return [primary];
    return [primary, ...additionalCharacters.map(c => ({ id: c.id, name: c.name, tagline: c.description || '', avatarUrl: c.avatarUrl, avatarColor: c.avatarColor, tags: [] }))];
  });
  let isMultiChar = $derived(allCards.length > 1);
  function nextCard() { activeCardIndex = (activeCardIndex + 1) % allCards.length; }
  function prevCard() { activeCardIndex = (activeCardIndex - 1 + allCards.length) % allCards.length; }

  // Reset carousel when conversation changes
  $effect(() => {
    characterId;  // track
    activeCardIndex = 0;
  });

  function getTagStyle(tag: { label: string; color: string }): string {
    return `background: ${tag.color}1F; color: ${tag.color};`;
  }
</script>

<div class="char-carousel" class:multi={isMultiChar}>
  <div class="carousel-track" style="transform: translateX(-{activeCardIndex * 100}%)">
    {#each allCards as card, i (card.id ?? i)}
      <div class="carousel-slide">
        <div class="char-card" style="--card-accent: {card.avatarColor}">
          <div class="char-avatar-lg" style="background: linear-gradient(135deg, {card.avatarColor}, {card.avatarColor}cc)" aria-hidden="true">
            {#if card.avatarUrl}
              <img src={card.avatarUrl} alt={card.name} class="ctx-avatar-img" />
            {/if}
          </div>
          <span class="char-name-lg">{card.name}</span>
          {#if card.tagline}
            <span class="char-tagline">{card.tagline}</span>
          {/if}
          <div class="char-tags">
            {#if card.tags && card.tags.length > 0}
              {#each card.tags as tag (tag.label)}
                <span class="tag" style={getTagStyle(tag)}>{tag.label}</span>
              {/each}
            {:else if i === 0}
              <span class="tag tag-violet">Fantasy</span>
              <span class="tag tag-pink">Mystery</span>
              <span class="tag tag-cyan">Magic</span>
            {/if}
          </div>
        </div>
      </div>
    {/each}
  </div>

  {#if isMultiChar}
    <div class="carousel-nav">
      <button class="carousel-nav-btn" onclick={prevCard} aria-label="Previous character">
        <Icon name="chevron-left" size={12} color="#8b8ba7" />
      </button>
      <div class="carousel-dots">
        {#each allCards as card, i}
          <button class="carousel-dot" class:active={activeCardIndex === i}
            onclick={() => activeCardIndex = i}
            aria-label="View {card.name}">
            {#if card.avatarUrl}
              <img src={card.avatarUrl} alt="" class="dot-avatar" />
            {:else}
              <div class="dot-color" style="background: {card.avatarColor}"></div>
            {/if}
          </button>
        {/each}
      </div>
      <button class="carousel-nav-btn" onclick={nextCard} aria-label="Next character">
        <Icon name="chevron-right" size={12} color="#8b8ba7" />
      </button>
    </div>
  {/if}
</div>

<style>
  /* ══ Character Carousel ══ */
  .char-carousel {
    position: relative;
    overflow: hidden;
    border-radius: 16px;
    background: rgba(14,14,30,0.6);
    border: 1px solid rgba(139,92,246,0.1);
    width: 100%;
    flex-shrink: 0;
  }
  .char-carousel.multi {
    border-color: rgba(0,212,224,0.12);
    background: rgba(10,14,28,0.7);
  }

  .carousel-track {
    display: flex;
    transition: transform 400ms cubic-bezier(0.4, 0, 0.2, 1);
    will-change: transform;
  }
  .carousel-slide {
    min-width: 100%; width: 100%; max-width: 100%; flex-shrink: 0;
  }

  .char-card {
    display: flex; flex-direction: column; align-items: center;
    gap: 14px; padding: 24px 16px 20px;
    position: relative; width: 100%; max-width: 100%;
    box-sizing: border-box;
  }
  .char-card::before {
    content: ''; position: absolute; top: -40px; left: 50%; transform: translateX(-50%);
    width: 120px; height: 120px; border-radius: 50%;
    background: radial-gradient(circle, rgba(139,92,246,0.15), transparent 70%);
    pointer-events: none;
  }

  /* Navigation Strip — compact bottom bar */
  .carousel-nav {
    display: flex; align-items: center; justify-content: center;
    gap: 10px; padding: 4px 0 12px;
  }
  .carousel-nav-btn {
    width: 24px; height: 24px; border-radius: 50%;
    border: 1px solid rgba(139,92,246,0.12);
    background: rgba(139,92,246,0.04);
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; transition: all 180ms ease-out;
    flex-shrink: 0; padding: 0;
  }
  .carousel-nav-btn:hover {
    border-color: rgba(0,212,224,0.3);
    background: rgba(0,212,224,0.08);
    transform: scale(1.1);
  }
  .carousel-nav-btn:active { transform: scale(0.9); }

  /* Avatar Dot Indicators */
  .carousel-dots {
    display: flex; align-items: center; gap: 8px;
  }
  .carousel-dot {
    width: 22px; height: 22px; border-radius: 50%;
    padding: 0; cursor: pointer; overflow: hidden;
    border: 2px solid rgba(139,92,246,0.12);
    background: transparent;
    transition: all 250ms ease-out;
    opacity: 0.5; transform: scale(0.85);
  }
  .carousel-dot:hover {
    opacity: 0.8; transform: scale(1);
    border-color: rgba(0,212,224,0.3);
  }
  .carousel-dot.active {
    opacity: 1; transform: scale(1.1);
    border-color: #00d4e0;
    box-shadow: 0 0 10px rgba(0,212,224,0.3);
  }
  .dot-avatar {
    width: 100%; height: 100%; object-fit: cover; display: block;
    border-radius: 50%;
  }
  .dot-color {
    width: 100%; height: 100%; border-radius: 50%;
  }

  .char-avatar-lg {
    width: clamp(64px, 18cqi, 104px); height: clamp(64px, 18cqi, 104px);
    min-width: 64px; min-height: 64px;
    border-radius: 50%; aspect-ratio: 1;
    background: linear-gradient(135deg, #8B5CF6, #bf40ff);
    overflow: hidden; position: relative; flex-shrink: 0;
    box-shadow: 0 0 20px rgba(139,92,246,0.25);
    transition: box-shadow 400ms;
  }
  .ctx-avatar-img { width: 100%; height: 100%; object-fit: cover; display: block; border-radius: 50%; }
  .char-name-lg { font-size: clamp(16px, 4.2cqi, 21px); font-weight: 700; color: #e8e0ff; }
  .char-tagline { font-size: clamp(11px, 2.8cqi, 15px); color: #6b6b8a; text-align: center; line-height: 1.5; }

  .char-tags { display: flex; gap: 6px; flex-wrap: wrap; justify-content: center; }
  .tag {
    padding: clamp(4px, 1cqi, 6px) clamp(10px, 2.4cqi, 14px); border-radius: 99px;
    font-size: clamp(10px, 2.6cqi, 13px); font-weight: 600; letter-spacing: 0.3px;
  }
  .tag-violet { background: rgba(139,92,246,0.12); color: #c4a1ff; }
  .tag-pink { background: rgba(191,64,255,0.12); color: #d580ff; }
  .tag-cyan { background: rgba(0,242,255,0.12); color: #00f2ff; }
</style>
