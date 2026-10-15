// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The 13 Help pages (Linux wording) and a small Markdown → Pango markup renderer.

pub struct Topic {
    pub id: &'static str,
    pub title: String,
    pub markdown: &'static str,
}

macro_rules! page {
    ($id:literal) => {
        ($id, include_str!(concat!("../help/", $id, ".md")))
    };
}

const PAGES: [(&str, &str); 13] = [
    page!("01-getting-started"),
    page!("02-visits"),
    page!("03-reducing"),
    page!("04-review-commit"),
    page!("05-restore"),
    page!("06-live-photos"),
    page!("07-keyboard-shortcuts"),
    page!("08-install-update"),
    page!("09-privacy"),
    page!("10-troubleshooting"),
    page!("11-about"),
    page!("12-blurry-photos"),
    page!("13-videos"),
];

/// All topics in order; the title is the page's first `# ` heading.
pub fn topics() -> Vec<Topic> {
    PAGES
        .iter()
        .map(|(id, md)| Topic {
            id,
            title: md
                .lines()
                .find_map(|l| l.strip_prefix("# "))
                .unwrap_or(id)
                .trim()
                .to_string(),
            markdown: md,
        })
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Inline Markdown: `**bold**`, `*italic*`, `` `code` ``, and bare `https://` links.
pub fn inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("**") {
            if let Some(end) = r.find("**") {
                out.push_str(&format!("<b>{}</b>", inline(&r[..end])));
                rest = &r[end + 2..];
                continue;
            }
        }
        if let Some(r) = rest.strip_prefix('`') {
            if let Some(end) = r.find('`') {
                out.push_str(&format!("<tt>{}</tt>", escape(&r[..end])));
                rest = &r[end + 1..];
                continue;
            }
        }
        if let Some(r) = rest.strip_prefix('*') {
            if let Some(end) = r.find('*').filter(|e| *e > 0) {
                out.push_str(&format!("<i>{}</i>", inline(&r[..end])));
                rest = &r[end + 1..];
                continue;
            }
        }
        if rest.starts_with("https://") {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ')')
                .unwrap_or(rest.len());
            let url = rest[..end].trim_end_matches(['.', ',']);
            out.push_str(&format!("<a href=\"{0}\">{0}</a>", escape(url)));
            rest = &rest[url.len()..];
            continue;
        }
        let c = rest.chars().next().unwrap();
        out.push_str(&escape(&c.to_string()));
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Block-level rendering to Pango markup (headings, bullets, numbered lists, paragraphs).
pub fn to_pango(md: &str) -> String {
    let mut blocks: Vec<String> = vec![];
    let mut para: Vec<String> = vec![];
    let flush = |para: &mut Vec<String>, blocks: &mut Vec<String>| {
        if !para.is_empty() {
            blocks.push(inline(&para.join(" ")));
            para.clear();
        }
    };
    for line in md.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            flush(&mut para, &mut blocks);
        } else if let Some(h) = t.strip_prefix("# ") {
            flush(&mut para, &mut blocks);
            blocks.push(format!(
                "<span size=\"x-large\" weight=\"bold\">{}</span>",
                inline(h)
            ));
        } else if let Some(h) = t.strip_prefix("## ") {
            flush(&mut para, &mut blocks);
            blocks.push(format!(
                "<span size=\"large\" weight=\"bold\">{}</span>",
                inline(h)
            ));
        } else if let Some(b) = t.strip_prefix("- ") {
            flush(&mut para, &mut blocks);
            blocks.push(format!("  •  {}", inline(b)));
        } else if let Some((n, rest)) = t
            .split_once(". ")
            .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        {
            flush(&mut para, &mut blocks);
            blocks.push(format!("  {n}.  {}", inline(rest)));
        } else {
            para.push(t.trim().to_string());
        }
    }
    flush(&mut para, &mut blocks);
    // Keep list items tight, separate other blocks with a blank line.
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            let list = |s: &str| s.starts_with("  ");
            out.push_str(if list(b) && list(&blocks[i - 1]) {
                "\n"
            } else {
                "\n\n"
            });
        }
        out.push_str(b);
    }
    out
}

/// `--help-selftest`: every topic renders to markup that has a title and no raw Markdown.
pub fn selftest() -> Result<usize, String> {
    let t = topics();
    if t.len() != 13 {
        return Err(format!("expected 13 help topics, found {}", t.len()));
    }
    for topic in &t {
        let m = to_pango(topic.markdown);
        if m.contains("**") || !m.contains("<span") {
            return Err(format!("help topic {} did not render", topic.id));
        }
    }
    Ok(t.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleven_topics_with_titles() {
        let t = topics();
        assert_eq!(t.len(), 13);
        assert_eq!(t[0].title, "Getting Started");
        assert_eq!(t[10].title, "About Unswiper");
        assert_eq!(selftest(), Ok(13));
    }

    #[test]
    fn linux_wording() {
        for t in topics() {
            for mac in [
                "⌘",
                "Finder",
                "System Settings",
                "Library/Application Support",
                ".dmg",
                "Dock",
            ] {
                assert!(!t.markdown.contains(mac), "{} mentions {mac}", t.id);
            }
        }
        assert!(topics()[8].markdown.contains("~/.local/share/unswiper"));
    }

    #[test]
    fn inline_markup() {
        assert_eq!(
            inline("a **b** `c<d>` *e* & f"),
            "a <b>b</b> <tt>c&lt;d&gt;</tt> <i>e</i> &amp; f"
        );
        assert_eq!(
            inline("see https://x.y/z."),
            "see <a href=\"https://x.y/z\">https://x.y/z</a>."
        );
        assert_eq!(inline("**unterminated"), "**unterminated");
        assert_eq!(inline("2 * 3"), "2 * 3");
        assert_eq!(inline("`open"), "`open");
        assert_eq!(inline("Ünïcode ☰"), "Ünïcode ☰");
    }

    #[test]
    fn blocks() {
        let m = to_pango("# T\n\nPara one\ncontinues.\n\n## H\n- a\n- b\n1. x\n2. y\n");
        assert_eq!(
            m,
            "<span size=\"x-large\" weight=\"bold\">T</span>\n\nPara one continues.\n\n<span size=\"large\" weight=\"bold\">H</span>\n\n  •  a\n  •  b\n  1.  x\n  2.  y"
        );
        assert_eq!(to_pango("3D. not a list"), "3D. not a list");
    }
}
