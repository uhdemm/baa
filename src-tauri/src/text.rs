//! Turns the model's raw transcript into what gets pasted: filler words out,
//! your replacements and snippets in, per-app style applied.

use crate::settings::{AppProfile, Settings};
use regex::{Regex, RegexBuilder};
use std::sync::OnceLock;

fn fillers() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(r"\b(?:u+m+|u+h+|e+r+m*|a+h+|h+m+|mm+)\b[,.]?[ \t]*")
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
    t = tidy_lines(&t);
    // a space keeps the next dictation from running into this one; multi-line text
    // (paragraphs, lists) ends where it ends
    if s.trailing_space && !t.is_empty() && !t.contains('\n') {
        t.push(' ');
    }
    t
}

/// Collapses runs of spaces inside each line but keeps the line structure:
/// paragraphs, list items and at most one blank line between blocks.
pub fn tidy_lines(t: &str) -> String {
    let mut out: Vec<String> = vec![];
    for line in t.lines() {
        let l = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() && out.last().map_or(true, |p| p.is_empty()) {
            continue; // no leading or doubled blank lines
        }
        out.push(l);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Turns Baa's plain output (paragraphs, "- " bullets, "1. " steps) into HTML so
/// rich-text apps get real lists, like Wispr Flow pastes them. None for one-liners.
pub fn to_html(t: &str) -> Option<String> {
    if !t.contains('\n') {
        return None;
    }
    let num = |l: &str| -> Option<String> {
        let (n, rest) = l.split_once(". ")?;
        n.chars().all(|c| c.is_ascii_digit()).then(|| rest.to_string())
    };
    let mut html = String::new();
    let mut open: Option<&str> = None; // "ul" | "ol" | "p"
    let close = |html: &mut String, open: &mut Option<&str>| {
        if let Some(tag) = open.take() {
            html.push_str(&format!("</{tag}>"));
        }
    };
    for line in t.lines() {
        let l = line.trim();
        let (tag, body) = if let Some(b) = l.strip_prefix("- ").or_else(|| l.strip_prefix("• ")).or_else(|| l.strip_prefix("* ")) {
            ("ul", Some(b.to_string()))
        } else if let Some(b) = num(l) {
            ("ol", Some(b))
        } else if l.is_empty() {
            ("", None)
        } else {
            ("p", Some(l.to_string()))
        };
        match (tag, body) {
            ("", _) => close(&mut html, &mut open),
            ("p", Some(b)) => {
                if open == Some("p") {
                    html.push_str(&format!("<br>{}", esc(&b)));
                } else {
                    close(&mut html, &mut open);
                    html.push_str(&format!("<p>{}", esc(&b)));
                    open = Some("p");
                }
            }
            (list, Some(b)) => {
                if open != Some(list) {
                    close(&mut html, &mut open);
                    html.push_str(&format!("<{list}>"));
                    open = Some(list);
                }
                html.push_str(&format!("<li>{}</li>", esc(&b)));
            }
            _ => {}
        }
    }
    close(&mut html, &mut open);
    Some(html)
}

/// Apps that render pasted HTML lists nicely (Word, Outlook, Gmail, Docs, Notion...).
/// Chat apps, code editors, terminals, Notepad and AI chat boxes get plain text.
pub fn wants_rich_text(app: Option<&str>, title: Option<&str>) -> bool {
    let app = app.unwrap_or("").to_lowercase();
    let title = title.unwrap_or("").to_lowercase();
    const PLAIN_APPS: &[&str] = &[
        "discord", "slack", "teams", "whatsapp", "telegram", "signal", "messenger", "code.exe", "cursor", "devenv",
        "idea64", "pycharm", "windowsterminal", "powershell", "cmd.exe", "wezterm", "alacritty", "notepad", "claude",
        "chatgpt",
    ];
    const PLAIN_SITES: &[&str] = &["discord", "slack", "whatsapp", "messenger", "chatgpt", "claude", "gemini", "x.com", "twitter", "reddit"];
    if PLAIN_APPS.iter().any(|a| app.contains(a)) {
        return false;
    }
    let browser = ["chrome", "msedge", "firefox", "brave", "opera", "arc", "vivaldi"].iter().any(|b| app.contains(b));
    !(browser && PLAIN_SITES.iter().any(|s| title.contains(s)))
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
    fn keeps_paragraphs_and_lists() {
        let raw = "Jonah wants a game.  


 To do:
- Clean the windows
-  Fix the house

";
        assert_eq!(process(raw, &base(), None), "Jonah wants a game.

To do:
- Clean the windows
- Fix the house");
    }

    #[test]
    fn filler_removal_keeps_line_breaks() {
        assert_eq!(process("So um
next line", &base(), None), "So
next line");
    }

    #[test]
    fn html_lists_and_paragraphs() {
        let t = "Jonah wants a game.
It's for Friday.

To do:
- Clean the windows
- Fix <the> house

1. First
2. Second";
        assert_eq!(
            to_html(t).unwrap(),
            "<p>Jonah wants a game.<br>It's for Friday.</p><p>To do:</p><ul><li>Clean the windows</li><li>Fix &lt;the&gt; house</li></ul><ol><li>First</li><li>Second</li></ol>"
        );
        assert!(to_html("one line").is_none());
    }

    #[test]
    fn rich_text_only_where_it_renders() {
        assert!(wants_rich_text(Some("winword.exe"), None));
        assert!(wants_rich_text(Some("chrome.exe"), Some("Inbox - Gmail")));
        assert!(!wants_rich_text(Some("discord.exe"), None));
        assert!(!wants_rich_text(Some("chrome.exe"), Some("ChatGPT - Google Chrome")));
    }

    #[test]
    fn per_app_lowercase() {
        let p = AppProfile { exe: "discord.exe".into(), auto_send: None, style: Some("lowercase".into()) };
        assert_eq!(process("Hey, what's up.", &base(), Some(&p)), "hey, what's up");
    }
}
