import { useState } from "react";
import type { Style as S, Styles } from "../api";
import type { PageProps } from "../App";

const CATS: { id: keyof Styles; label: string; desc: string; apps: string }[] = [
  { id: "personal_chat", label: "Personal messages", desc: "Friends and family", apps: "Discord, WhatsApp, Telegram, Messenger, Signal" },
  { id: "work_chat", label: "Work messages", desc: "Team chat", apps: "Slack, Teams, Zoom chat" },
  { id: "email", label: "Email", desc: "Inbox and replies", apps: "Outlook, Gmail, Thunderbird" },
  { id: "other", label: "Everything else", desc: "Docs, notes, AI, search", apps: "Word, Notion, ChatGPT, Claude, your browser" },
];

const STYLES: { id: S; label: string; desc: string }[] = [
  { id: "formal", label: "Formal", desc: "Capitals and full punctuation" },
  { id: "casual", label: "Casual", desc: "Capitals, lighter punctuation" },
  { id: "very_casual", label: "Very casual", desc: "no caps, barely any punctuation" },
  { id: "excited", label: "Excited", desc: "More energy!" },
];

// the same sentence in each style, per category
const SAMPLE: Record<keyof Styles, Record<S, string>> = {
  personal_chat: {
    formal: "Hey, are you free tonight? I was thinking we could get dinner.",
    casual: "Hey are you free tonight? I was thinking we could get dinner",
    very_casual: "hey are you free tonight? i was thinking we could get dinner",
    excited: "Hey are you free tonight? I was thinking we could get dinner!",
  },
  work_chat: {
    formal: "The render is done. I'll upload it to the drive before lunch.",
    casual: "The render is done, I'll upload it to the drive before lunch",
    very_casual: "the render is done ill upload it to the drive before lunch",
    excited: "The render is done! I'll upload it to the drive before lunch",
  },
  email: {
    formal: "Hi Sarah,\n\nThanks for sending these over. They look good to me.\n\nBest,\nAdam",
    casual: "Hi Sarah,\n\nThanks for sending these over, they look good to me\n\nAdam",
    very_casual: "hi sarah\n\nthanks for sending these over they look good to me\n\nadam",
    excited: "Hi Sarah,\n\nThanks for sending these over! They look great to me.\n\nAdam",
  },
  other: {
    formal: "The project will take about six weeks and cost $12,500.",
    casual: "The project will take about six weeks and cost $12,500",
    very_casual: "the project will take about six weeks and cost $12,500",
    excited: "The project will take about six weeks and cost $12,500!",
  },
};

export default function Style({ settings, save }: PageProps) {
  const [cat, setCat] = useState<keyof Styles>("personal_chat");
  const styles = settings.styles;
  const pick = (s: S) => save({ ...settings, styles: { ...styles, [cat]: s } });
  const ai = settings.ai.cleanup;
  const current = CATS.find((c) => c.id === cat)!;

  return (
    <>
      <div className="page-head">
        <h1>Style</h1>
        <p>How you sound in each kind of app. Baa works out which kind you're typing into.</p>
      </div>
      {!ai && (
        <div className="banner">
          Without AI clean-up only capitals and the final period change. Turn on clean-up in AI for the full style.
        </div>
      )}
      <div className="seg" role="tablist">
        {CATS.map((c) => (
          <button key={c.id} role="tab" className={cat === c.id ? "on" : ""} onClick={() => setCat(c.id)}>{c.label}</button>
        ))}
      </div>
      <p className="hint">{current.desc}: {current.apps}</p>
      <div className="style-grid">
        {STYLES.map((s) => (
          <button key={s.id} className={`style-card ${styles[cat] === s.id ? "on" : ""}`} onClick={() => pick(s.id)}>
            <div className="style-top">
              <b>{s.label}</b>
              <span>{s.desc}</span>
            </div>
            <p className="style-sample">{SAMPLE[cat][s.id]}</p>
          </button>
        ))}
      </div>
    </>
  );
}
