"""Run Baa's meeting-notes prompt on a sample diarized call and print the result.

    python scripts/test_meeting.py
Reads ANTHROPIC_API_KEY from C:/Users/kong1/after-rejects/.env (never printed).
Same request shape as the app: prompt as system; template, attendees, my notes, transcript.
"""
import pathlib

import anthropic
from test_cleanup import key

ROOT = pathlib.Path(__file__).resolve().parent.parent
PROMPT = (ROOT / "src-tauri" / "src" / "meeting_prompt.txt").read_text(encoding="utf-8")

# misheard names ("brat", "sara") and nobody says who Speaker 3 is
TRANSCRIPT = """[0:02] Speaker 1: hey adam hey sara can you both hear me okay
[0:06] Me: yeah loud and clear
[0:08] Speaker 2: yep all good thanks brat
[0:12] Speaker 1: cool so the hotbox shoot friday is still on we start at nine thirty at the studio
[0:20] Speaker 2: I need the final shot list by wednesday though otherwise I can't book the stylist
[0:27] Me: I'll send the shot list tomorrow and I'll bring the two softboxes and the tripod
[0:35] Speaker 3: and the budget is still twelve thousand five hundred right
[0:39] Speaker 1: yes twelve five, sara can you confirm the talent by thursday
[0:44] Speaker 2: on it"""


def main():
    client = anthropic.Anthropic(api_key=key())
    user = (
        "<template>general</template>\n<attendees>Brad, Sarah from HotBox, Jona</attendees>\n"
        "<my_notes>\nbudget??\n</my_notes>\n<transcript>\n" + TRANSCRIPT + "\n</transcript>"
    )
    msg = client.messages.create(
        model="claude-opus-5-5", max_tokens=4096, system=PROMPT, output_config={"effort": "low"},
        messages=[{"role": "user", "content": user}],
    )
    print("".join(b.text for b in msg.content if b.type == "text"))


if __name__ == "__main__":
    main()
