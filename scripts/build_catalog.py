"""Build Baa's speech model catalog from Hugging Face.

    python -I scripts/build_catalog.py

Lists the public GGUF speech-model repos that transcribe.cpp can load, reads each
repo's files and sizes from the Hugging Face API, and writes
src-tauri/src/catalog.json (embedded into the app at build time). Only the
Q8_0 file per model is listed: the best size/quality trade for dictation.
"""
import json
import pathlib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "src-tauri" / "src" / "catalog.json"
API = "https://huggingface.co/api"

# Our curated picks: id -> (display name, blurb, tier). Order = display order.
PICKS = {
    "parakeet-tdt-0.6b-v3": ("Parakeet v3", "Fast and accurate. 25 European languages, detects the language itself.", "recommended"),
    "parakeet-unified-en-0.6b": ("Parakeet English", "Best English accuracy for its size. English only.", "recommended"),
    "whisper-large-v3-turbo": ("Whisper Turbo", "Whisper's best speed/accuracy mix. 99 languages. Needs a GPU to feel quick.", "great"),
    "canary-1b-v2": ("Canary 1B v2", "Very accurate, 25 languages, can translate to English.", "great"),
    "Qwen3-ASR-0.6B": ("Qwen3-ASR 0.6B", "Strong multilingual model incl. Chinese, Japanese, Korean.", "great"),
    "whisper-small": ("Whisper Small", "Small and dependable. 99 languages.", "light"),
    "parakeet-tdt_ctc-110m": ("Parakeet Mini", "Tiny and quick. English. Good for older laptops.", "light"),
    "canary-180m-flash": ("Canary Flash", "Tiny, fast, 4 languages.", "light"),
    "moonshine-base": ("Moonshine", "The smallest. English. Runs on anything.", "light"),
    "whisper-medium": ("Whisper Medium", "Older Whisper, accurate but slow on CPU.", "more"),
    "whisper-large-v3": ("Whisper Large v3", "Biggest Whisper. Needs a strong GPU.", "more"),
    "Voxtral-Mini-3B-2507": ("Voxtral Mini 3B", "Large and very accurate. Needs a strong GPU.", "more"),
    "granite-speech-4.1-2b": ("Granite Speech 2B", "IBM's accurate English/European model. Needs a GPU.", "more"),
    "SenseVoiceSmall": ("SenseVoice Small", "Fast, great for Chinese, Cantonese, Japanese, Korean.", "more"),
}
ORG = "handy-computer"  # public org hosting transcribe.cpp GGUF conversions
QUANT = "Q8_0"
VAD = ("silero-vad-gguf", "silero-vad-v6.2-F32.gguf")


def get(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "baa-catalog"}), timeout=30) as r:
        return json.load(r)


def main():
    models = []
    for mid, (name, blurb, tier) in PICKS.items():
        repo = f"{ORG}/{mid}-gguf"
        try:
            info = get(f"{API}/models/{repo}?blobs=true")
        except Exception as e:
            print("skip", repo, e)
            continue
        files = {s["rfilename"]: s for s in info.get("siblings", [])}
        pick = next((f for f in files if f.endswith(f"-{QUANT}.gguf")), None) or next((f for f in files if f.endswith(".gguf")), None)
        if not pick:
            print("no gguf in", repo)
            continue
        f = files[pick]
        models.append({
            "id": mid, "name": name, "description": blurb, "tier": tier,
            "repo": repo, "revision": info["sha"], "filename": pick,
            "size": f.get("size") or (f.get("lfs") or {}).get("size", 0),
            "sha256": (f.get("lfs") or {}).get("sha256"),
            "license": (info.get("cardData") or {}).get("license"),
        })
        print(f"{mid}: {pick} {models[-1]['size'] / 1e6:.0f} MB")
    vad_repo = f"{ORG}/{VAD[0]}"
    try:
        info = get(f"{API}/models/{vad_repo}?blobs=true")
        f = next(s for s in info["siblings"] if s["rfilename"] == VAD[1])
        vad = {"repo": vad_repo, "revision": info["sha"], "filename": VAD[1],
               "size": f.get("size", 0), "sha256": (f.get("lfs") or {}).get("sha256")}
        print("vad:", vad["filename"])
    except Exception as e:
        print("no vad repo:", e)
        vad = None
    OUT.write_text(json.dumps({"models": models, "vad": vad}, indent=1), encoding="utf-8")
    print("wrote", OUT, len(models), "models")


if __name__ == "__main__":
    main()
