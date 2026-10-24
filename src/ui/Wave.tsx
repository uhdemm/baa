// Soft rounded waveform. Static styles match the Figma "Waveform" set; "live" follows the mic level.
import { useEffect, useRef, useState } from "react";

const SHAPES = {
  listening: [5, 7, 11, 9, 15, 22, 28, 19, 14, 11, 9, 7, 5, 3, 3, 3, 3, 3],
  calm: [3, 5, 8, 11, 16, 22, 16, 12, 9, 7, 5, 3, 3],
  focused: [5, 9, 14, 22, 30, 34, 24, 18, 14, 10, 8, 6, 4, 3],
  energetic: [5, 9, 7, 14, 11, 22, 30, 18, 24, 12, 9, 6, 4, 3],
};
// bars after this index fade to the soft green
const FADE_FROM = { listening: 11, calm: 10, focused: 11, energetic: 11 };

type Style = keyof typeof SHAPES | "processing" | "idle";

export default function Wave({ style = "listening", level }: { style?: Style; level?: number }) {
  if (style === "idle") return <div className="wave dots">{Array.from({ length: 15 }, (_, i) => <i key={i} className="dot" />)}</div>;
  if (style === "processing") return <Processing />;
  const shape = SHAPES[style];
  const fade = FADE_FROM[style];
  return (
    <div className="wave">
      {shape.map((h, i) => {
        const live = level === undefined ? h : Math.max(3, h * (0.25 + level * 1.15) * (0.85 + 0.3 * Math.sin(i * 1.7 + Date.now() / 160)));
        return <i key={i} className={i >= fade ? "soft" : ""} style={{ height: Math.min(34, live) }} />;
      })}
    </div>
  );
}

// eight dots with a green sweep moving through them
function Processing() {
  const [t, setT] = useState(0);
  const raf = useRef(0);
  useEffect(() => {
    const id = window.setInterval(() => setT((v) => v + 1), 120);
    return () => { window.clearInterval(id); cancelAnimationFrame(raf.current); };
  }, []);
  return (
    <div className="wave dots processing">
      {Array.from({ length: 8 }, (_, i) => {
        const d = (t - i + 64) % 8;
        return <i key={i} className={`dot ${d < 5 ? "on" : d < 6 ? "half" : ""}`} />;
      })}
    </div>
  );
}
