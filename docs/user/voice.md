# Voice

Janus can read replies aloud, with a different voice for each character. Voice is **off by default**: nothing speaks until you turn it on.

## Turning it on

1. Open **Settings, Voice** and switch on **Enable Voice Output**.
2. Pick a **Voice provider**.
   - **Kokoro (built-in, offline)** runs entirely on your computer with no account. It needs a one-time download of the voice model (about 90 MB). Settings, Voice shows **Voice Model: Not downloaded** with a download button and progress.
   - A cloud provider (**ElevenLabs**, **Google Cloud TTS** or **Fish Audio**) uses your own API key. Add it first under **Providers** with type **Voice** (see [Providers and models](providers-and-models.md)).
3. Choose a **Default voice**, used for any character without one of their own.

```{note}
The built-in Kokoro voice currently downloads the Windows build of its speech runtime, so it works on **Windows only** for now. On macOS and Linux, use a cloud voice provider.
```

## How it sounds

With the built-in Kokoro voice, replies are spoken **sentence by sentence while they are being written**, so audio starts long before the reply finishes. With a cloud voice, the reply is saved first and spoken as a whole when you press the play control (live streaming for cloud voices is not available yet). Every message has a play control (a waveform button) to replay it. A speaker button in the title bar stops playback.

## Voices per character

Open a character's profile, press **Edit Character**, choose the **Voice provider** and **Voice** for them, and use the speaker button to preview. Characters without a voice use your default.

## The Voice Models page

**AI Studio, Voice Models** lists every voice from every configured source (the built-in Kokoro voices, and your cloud providers' voices) with an inline preview, so you can audition a voice before assigning it. If Kokoro is not downloaded it tells you to fetch it in Settings, Voice.

## Notes

- Kokoro ships 54 voices and uses roughly one second of computing for every two seconds of speech on an ordinary CPU. The engine loads when needed, and Janus warms it up at startup when voice is on.
- Cloud voices send the text of replies to that provider. Turn voice off, or keep to Kokoro, if you want nothing leaving your computer.
- Replaying a message's voice works in the desktop app; it is not available from a browser yet.
- The Kokoro model is shared by everyone on this computer. Downloading it and the resource monitor are managed from the desktop app, not from a browser.
