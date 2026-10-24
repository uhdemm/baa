//! Turns the model's raw transcript into what gets pasted: filler words out,
//! your replacements and snippets in, per-app style applied.

use crate::settings::{AppProfile, Settings};
use regex::{Regex, RegexBuilder};
use std::sync::OnceLock;

fn fillers() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(r"\b(?:u+m+|u+h+|e+r+m*|a+h+|h+m+|mm+)\b[,.]?\s*")
            .case_insensitive(true)
            .build()
            .unwrap()
    })
}

fn word_re(phrase: &str) -> Option<Regex> {
    let p = phrase.trim();
    if p.is_empty() {
        return None;
    }
    RegexBuilder::new(&format!(r"\b{}\b", regex::escape(p))).case_insensitive(true).build().ok()
}

/// Lowercase, letters/digits/spaces only: how a spoken trigger is compared.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn process(raw: &str, s: &Settings, profile: Option<&AppProfile>) -> String {
    let mut t = raw.trim().to_string();

    // A dictation that is only a snippet trigger becomes the snippet text as-is.
    let n = norm(&t);
    if let Some(sn) = s.snippets.iter().find(|sn| !sn.trigger.trim().is_empty() && norm(&sn.trigger) == n) {
        return sn.text.clone();
    }

    if s.remove_fillers {
        let started_upper = t.chars().next().is_some_and(char::is_uppercase);
        t = fillers().replace_all(&t, "").to_string();
        // "Um, so I think" -> "So I think": keep the capital the sentence had
        if started_upper {
            let mut c = t.chars();
            if let Some(f) = c.next() {
                t = f.to_uppercase().collect::<String>() + c.as_str();
            }
        }
    }
    for r in &s.replacements {
        if let Some(re) = word_re(&r.from) {
            t = re.replace_all(&t, regex::NoExpand(&r.to)).to_string();
        }
    }
    // Snippet triggers said inside a longer sentence expand in place.
    for sn in &s.snippets {
        if let Some(re) = word_re(&sn.trigger) {
            t = re.replace_all(&t, regex::NoExpand(&sn.text)).to_string();
        }
    }

    match profile.and_then(|p| p.style.as_deref()) {
        Some("lowercase") => {
            t = t.to_lowercase();
            t = t.trim_end_matches('.').to_string();
        }
        Some("no_period") => t = t.trim_end_matches('.').to_string(),
        _ => {}
    }
    t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.trailing_space && !t.is_empty() {
        t.push(' ');
    }
    t
}

pub fn word_count(t: &str) -> usize {
    t.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Replacement, Snippet};

    fn base() -> Settings {
        Settings { trailing_space: false, ..Default::default() }
    }

    #[test]
    fn fillers_go() {
        assert_eq!(process("Um, so I think uh we should go.", &base(), None), "So I think we should go.");
    }

    #[test]
    fn replacement_and_snippet() {
        let mut s = base();
        s.replacements.push(Replacement { from: "big square".into(), to: "Big Square".into() });
        s.snippets.push(Snippet { trigger: "my email".into(), text: "adam@example.com".into() });
        assert_eq!(process("send it to big square", &s, None), "send it to Big Square");
        assert_eq!(process("My email.", &s, None), "adam@example.com");
        assert_eq!(process("it's my email okay", &s, None), "it's adam@example.com okay");
    }

    #[test]
    fn per_app_lowercase() {
        let p = AppProfile { exe: "discord.exe".into(), auto_send: None, style: Some("lowercase".into()) };
        assert_eq!(process("Hey, what's up.", &base(), Some(&p)), "hey, what's up");
    }
}
