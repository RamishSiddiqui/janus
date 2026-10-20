# Emotions and scenes

Janus tracks two kinds of story state beside the text: how each character **feels**, and **where** the scene is. Both are inferred automatically after each reply, shown in the chat, and fed back into later prompts so the story stays consistent.

## Emotional state

After a reply finishes, a short background call to the model estimates how the speaking character feels, using the previous state as a baseline so moods change gradually instead of flipping.

| Measure | Range | Meaning |
|---|---|---|
| **Mood** | 0 to 100 | From devastated (0) through neutral (50) to elated (100). |
| **Trust** | 0 to 100 | From hostile (0) through wary (50) to devoted (100). |
| **Intensity** | 0 to 100 | From withdrawn (0) through engaged (50) to intense (100). |
| **Dominant emotion** | one word | For example curious, guarded, tender, melancholy. |
| **Summary** | a sentence or two | The state in plain language. |

You see these in the **emotional state** panel beside the chat, and as a small mood tag on each reply, which is a snapshot of how they felt at that message. The current state is included in the next prompt, so a character who has grown wary stays wary.

## Scene state

The bar above the chat shows the current scene:

- **Location**
- **Present** (who is in the scene)
- **Ambience** (time of day, weather)
- **Mood** of the scene

Click the bar to expand it. A background check updates it after each reply by reading the story, and a chat's opening scene is extracted when the chat starts. The scene is added to the prompt so the model remembers where everyone is.

## Scene images and video

The **Scene** panel generates pictures (and short videos) of the current moment.

1. Add an **Image** (or **Video**) provider under [Providers](providers-and-models.md). AI Horde is free and needs no signup; ComfyUI and WanGP run locally.
2. In the Scene panel, switch between **Image** and **Video**, describe the scene or press **Click to generate** to build a prompt from the scene state, and generate. Use **Edit Prompt** to adjust the text first.
3. Optional controls, depending on the provider and model: **Style Preset** (a saved bundle of settings, see below), **Model**, **Duration** and frame rate for video, **Cast Portraits** (send confirmed characters' portraits to a ComfyUI workflow), and **Avatar Reference** with **Reference Strength** (use a portrait as a starting image; lower keeps its pose, higher gives the scene more freedom).
4. **Regenerate** tries again, **Save** keeps the result.

With **Auto-Generate Scene Images** on (Settings, Chat Behavior), a new image is made whenever the scene changes.

If no worker is available right now (AI Horde), the panel says it is still waiting. A **Stop** control cancels a generation.

### The scene gallery

The **Scene Gallery** icon in the chat header lists every generated scene in the chat. Open one to see exactly how it was made: **Prompt**, **Negative Prompt**, **Model**, **Style**, **Sampler**, **CFG Scale**, **Steps**, **Karras**, **Clip Skip**, **Size**, **Post-Processing**, **Hi-Res Fix**, **Img2Img Strength**, **Seed** and **Provider**. You can copy the prompts and delete scenes.

### Style presets

A preset is a saved bundle of sampler, CFG scale, steps, model, style, negative prompt and quality options. Create and edit them under **Settings, Image Generation**. Mark one as the default, or pick one per chat in the Scene panel. See [Settings](settings.md#image-generation).

### Mature content

**Allow Mature Content** (Settings, Image Generation) controls a flag sent to AI Horde. Off means strict filtering. It is on by default because ordinary character descriptions often trip the strict filter by accident. It does not make everything explicit.
