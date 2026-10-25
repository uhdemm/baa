# Baa

Voice typing for Windows. Hold a shortcut, talk, let go: your words land in whatever app you're typing in,
cleaned up and formatted for where they're going. Speech recognition runs on your own PC.

Made by Adam Selman.

## What it does

**Dictation**
- Hold `Ctrl+Space` (or tap it for hands-free), talk, let go. A mouse side button works too.
- Live words while you talk; `Esc` cancels.
- Local speech models (Parakeet, Whisper, Canary and more, as GGUF) on the GPU through
  [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp). Import your own model files.
- Whisper mode for quiet speech, speak any language and get English, mutes your music while you talk.
- Carries on the sentence you're in (reads the text before the cursor), learns words you correct.

**Clean-up (optional AI)**
- Fixes false starts, numbers, emails and punctuation, and formats like you'd type it: paragraphs, real bullet
  lists, emails with a greeting and sign-off. Pastes rich text where the app supports it.
- Knows where it's pasting (app + window) and how you sound there: formal, casual, very casual or excited per
  kind of app.
- Bring your own provider: Groq, Gemini, Cerebras, OpenRouter, Anthropic, OpenAI, Ollama or any
  OpenAI-compatible server. Keys live in Windows Credential Manager.
- Command mode: select text, hold `Alt+Shift+Space`, say what to change.

**Notes and meetings**
- Notes you can dictate into.
- Meeting notes: records you and the call, removes the call's echo from your mic so "Me" is only you, tells
  up to four other people apart (NVIDIA Sortformer), names them from the conversation, then writes the summary,
  decisions and action items. Ask questions about any note afterwards.

**Buddy**
- Instead of the little bar, a character can live on your taskbar: it mouths along while you talk, says your
  words in a speech bubble, throws a paper airplane when it sends, and dozes off when you're idle.
- **Character Studio**: describe any character (and attach pictures) and Baa draws it with every move it needs,
  cleans and lines up the frames, and installs it. Pixel-art mode included. Characters are shareable `.baa` files.

**Also:** per-app rules, words and snippets, history with insights and export, a macOS-style look you can
customise (material, thickness, tint, accent, roundness), tray app, starts with Windows.

## Build

Windows 10/11. Needs [Rust](https://rustup.rs), [Bun](https://bun.sh), the
[Vulkan SDK](https://vulkan.lunarg.com/sdk/home) and Ninja.

```powershell
bun install
. .\scripts\win-env.ps1      # Ninja + Vulkan SDK on PATH
bun run tauri dev           # run it
bun run tauri build         # installer in src-tauri/target/release/bundle/nsis
```

Character Studio draws with the [Codex CLI](https://github.com/openai/codex) and your ChatGPT sign-in
(`npm i -g @openai/codex`, then run `codex` once). Everything else works without it.

## Made with

[Tauri 2](https://tauri.app), React, Rust, [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp)
with Silero VAD. Character Studio adapts the pipeline from
[Sprite Studio](https://github.com/JohnKinyanjui/sprite-maker) by John Kinyanjui (MIT); see
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

## License

MIT, see [LICENSE](LICENSE).
