//! Auto-learn words: after a paste, look at the same text box a few times over the
//! next half minute (Windows UI Automation). If you replaced a word Baa wrote with
//! a similar-looking one ("promotion" -> "Remotion"), suggest adding your spelling.

/// Pure part: which words did the user swap in for words we pasted?
pub fn corrections(pasted: &str, before: &str, after: &str) -> Vec<String> {
    let words = |s: &str| -> Vec<String> {
        s.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '-'))
            .filter(|w| w.chars().count() >= 3)
            .map(str::to_string)
            .collect()
    };
    let pasted_w = words(pasted);
    let before_w: std::collections::HashSet<String> = words(before).into_iter().collect();
    let after_w: std::collections::HashSet<String> = words(after).into_iter().collect();
    let mut out = vec![];
    for gone in pasted_w.iter().filter(|w| !after_w.contains(*w)) {
        for new in after_w.iter().filter(|w| !before_w.contains(*w)) {
            if similarity(&gone.to_lowercase(), &new.to_lowercase()) >= 0.6 && !out.contains(new) {
                out.push(new.clone());
            }
        }
    }
    out
}

fn similarity(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a == b {
        return 0.0; // same word in a different case isn't a correction worth learning
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + (ca != cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f64 / a.len().max(b.len()) as f64
}

/// Watches the focused text box after a paste and calls `found` with corrections.
#[cfg(windows)]
pub fn watch(pasted: String, found: impl Fn(Vec<String>) + Send + 'static) {
    std::thread::spawn(move || unsafe {
        use windows::core::Interface;
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationValuePattern,
            UIA_TextPatternId, UIA_ValuePatternId,
        };
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        std::thread::sleep(std::time::Duration::from_millis(500)); // let the paste land
        let Ok(auto) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else { return };
        let Ok(el) = auto.GetFocusedElement() else { return };
        let read = |el: &IUIAutomationElement| -> Option<String> {
            if let Ok(p) = el.GetCurrentPattern(UIA_ValuePatternId) {
                if let Ok(v) = p.cast::<IUIAutomationValuePattern>() {
                    if let Ok(s) = v.CurrentValue() {
                        return Some(s.to_string());
                    }
                }
            }
            let p = el.GetCurrentPattern(UIA_TextPatternId).ok()?;
            let t = p.cast::<IUIAutomationTextPattern>().ok()?;
            t.DocumentRange().ok()?.GetText(20_000).ok().map(|s| s.to_string())
        };
        let Some(before) = read(&el) else { return };
        if !before.contains(pasted.trim()) {
            return; // can't see our text in this box (password field, canvas app...)
        }
        let mut reported: Vec<String> = vec![];
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let Some(now) = read(&el) else { return };
            let fresh: Vec<String> =
                corrections(&pasted, &before, &now).into_iter().filter(|w| !reported.contains(w)).collect();
            if !fresh.is_empty() {
                reported.extend(fresh.iter().cloned());
                found(fresh);
            }
        }
    });
}

#[cfg(not(windows))]
pub fn watch(_pasted: String, _found: impl Fn(Vec<String>) + Send + 'static) {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_a_fixed_name() {
        let pasted = "I rendered it in promotion today";
        let before = format!("Hey team. {pasted}");
        let after = "Hey team. I rendered it in Remotion today";
        assert_eq!(corrections(pasted, &before, after), vec!["Remotion".to_string()]);
    }
    #[test]
    fn ignores_unrelated_typing() {
        let pasted = "see you tomorrow";
        assert!(corrections(pasted, pasted, "see you tomorrow at the office").is_empty());
    }
}
