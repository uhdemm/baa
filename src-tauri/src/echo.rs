//! Keeps "Me" honest in meeting notes. Without headphones your mic also hears the
//! call coming out of your speakers, so the other people would be written down as
//! you. We have the speaker audio too, so for every 20 ms we check: is the mic just
//! a quieter, slightly late copy of what the speakers played (drop it), or clearly
//! louder than that echo could be (you talking, keep it, even over someone else)?

const FRAME: usize = 320; // 20 ms at 16 kHz
/// How late the echo can arrive after the speakers play it (300 ms).
const LAG_FRAMES: usize = 15;
/// Call audio below this is silence: whatever the mic hears then is you.
const CALL_QUIET: f32 = 0.004;
/// The mic has to beat the expected echo by this much to count as you.
const MARGIN: f32 = 2.5;
/// Keep a little either side of each kept stretch so words aren't clipped.
const HANGOVER: usize = 6;

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// Remembers how loud the echo usually is on this PC (speaker volume, mic distance)
/// across the 20-second slices of one meeting.
#[derive(Default)]
pub struct Gate {
    /// mic level / speaker level when only the call is talking
    gain: Option<f32>,
}

impl Gate {
    /// Returns the mic audio with the echo-only parts silenced. `mic` and `call`
    /// are the same stretch of time at 16 kHz.
    pub fn clean(&mut self, mic: &[f32], call: &[f32]) -> Vec<f32> {
        let n = mic.len() / FRAME + usize::from(mic.len() % FRAME != 0);
        let m: Vec<f32> = (0..n).map(|i| rms(&mic[i * FRAME..((i + 1) * FRAME).min(mic.len())])).collect();
        let c: Vec<f32> = (0..n)
            .map(|i| {
                let a = (i * FRAME).min(call.len());
                let b = ((i + 1) * FRAME).min(call.len());
                rms(&call[a..b])
            })
            .collect();
        // the loudest the speakers were over the last 300 ms: the echo can't be louder
        let env: Vec<f32> =
            (0..n).map(|i| c[i.saturating_sub(LAG_FRAMES)..=i].iter().cloned().fold(0.0, f32::max)).collect();

        // Echo gain: while the call talks, the mic is mostly just echo (you listen more
        // than you talk), so the typical mic/speaker ratio is the echo level.
        let mut ratios: Vec<f32> = (0..n).filter(|&i| env[i] > CALL_QUIET).map(|i| m[i] / env[i]).collect();
        if ratios.len() >= 25 {
            ratios.sort_by(|a, b| a.total_cmp(b));
            let median = ratios[ratios.len() / 2].min(2.0);
            self.gain = Some(match self.gain {
                Some(g) => g * 0.6 + median * 0.4,
                None => median,
            });
        }
        let g = self.gain.unwrap_or(0.5); // unknown yet: assume a loud echo, stay safe

        let mine: Vec<bool> = (0..n).map(|i| env[i] <= CALL_QUIET || m[i] > MARGIN * g * env[i]).collect();
        // hangover both ways, so the edges of your words survive
        let mut keep = vec![false; n];
        for i in (0..n).filter(|&i| mine[i]) {
            for k in keep.iter_mut().take((i + HANGOVER + 1).min(n)).skip(i.saturating_sub(HANGOVER)) {
                *k = true;
            }
        }
        let mut out = mic.to_vec();
        for (i, k) in keep.iter().enumerate() {
            if !k {
                let b = ((i + 1) * FRAME).min(out.len());
                out[i * FRAME..b].iter_mut().for_each(|s| *s = 0.0);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Speech": a tone that switches on and off every half second.
    fn bursts(len: usize, freq: f32, amp: f32, phase_s: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32 / 16000.0;
                if ((t + phase_s) * 2.0) as i64 % 2 == 0 {
                    amp * (t * freq * std::f32::consts::TAU).sin()
                } else {
                    0.0
                }
            })
            .collect()
    }
    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|s| s * s).sum()
    }
    fn delayed(x: &[f32], by: usize, gain: f32) -> Vec<f32> {
        (0..x.len()).map(|i| if i >= by { x[i - by] * gain } else { 0.0 }).collect()
    }

    #[test]
    fn drops_the_call_coming_back_through_the_mic() {
        let call = bursts(16000 * 10, 220.0, 0.3, 0.0);
        let mic = delayed(&call, 1600, 0.15); // 100 ms late, quieter
        let out = Gate::default().clean(&mic, &call);
        assert!(energy(&out) < energy(&mic) * 0.05, "echo should be gone");
    }

    #[test]
    fn keeps_you_when_the_call_is_quiet() {
        let call = vec![0.0; 16000 * 5];
        let mic = bursts(16000 * 5, 180.0, 0.2, 0.0);
        let out = Gate::default().clean(&mic, &call);
        assert!(energy(&out) > energy(&mic) * 0.99);
    }

    #[test]
    fn keeps_you_talking_over_someone() {
        let call = bursts(16000 * 10, 220.0, 0.3, 0.0);
        let echo = delayed(&call, 1600, 0.15);
        let mut g = Gate::default();
        g.clean(&echo, &call); // learn the echo level first
        // you speak in the second half of every burst cycle, on top of the echo
        let me = bursts(16000 * 10, 150.0, 0.25, 0.25);
        let mic: Vec<f32> = echo.iter().zip(&me).map(|(a, b)| a + b).collect();
        let out = g.clean(&mic, &call);
        assert!(energy(&out) > energy(&me) * 0.8, "your words must survive");
    }
}
