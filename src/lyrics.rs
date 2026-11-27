//! Synchronised lyrics from `.lrc` files (`[mm:ss.xx] line`). The file sits next to the song with
//! the same name (`song.mp3` → `song.lrc`). Experimental: shown on the desktop by the widget thread.

use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub struct Lyrics {
    /// (seconds, text) sorted by time; empty text = a pause.
    pub lines: Vec<(f64, String)>,
}

/// The `.lrc` next to a song, if there is one.
pub fn lrc_for(song: &Path) -> Option<PathBuf> {
    let p = song.with_extension("lrc");
    p.is_file().then_some(p)
}

pub fn load(song: &Path) -> Lyrics {
    lrc_for(song).and_then(|p| std::fs::read(&p).ok()).map(|b| parse(&String::from_utf8_lossy(&b))).unwrap_or_default()
}

pub fn parse(text: &str) -> Lyrics {
    let mut lines = Vec::new();
    let mut offset = 0.0f64;
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some((m, s)) = tag.split_once(':') {
                if let (Ok(m), Ok(s)) = (m.trim().parse::<f64>(), s.trim().replace(',', ".").parse::<f64>()) { times.push(m * 60.0 + s); }
                else if m.trim().eq_ignore_ascii_case("offset") { offset = s.trim().parse::<f64>().unwrap_or(0.0) / 1000.0; }
            }
            rest = rest[end + 1..].trim_start();
        }
        let content = rest.trim().to_string();
        for t in times { lines.push((t, content.clone())); }
    }
    for l in lines.iter_mut() { l.0 = (l.0 - offset).max(0.0); }
    lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Lyrics { lines }
}

impl Lyrics {
    pub fn is_empty(&self) -> bool { self.lines.is_empty() }

    /// Index of the line being sung at `pos` seconds.
    pub fn index_at(&self, pos: f64) -> Option<usize> {
        if self.lines.is_empty() || pos < self.lines[0].0 { return None; }
        Some(self.lines.partition_point(|l| l.0 <= pos).saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    #[test]
    fn lrc() {
        let l = parse("[ti:x]\n[offset:500]\n[00:10.00]hello\n[00:05.50][00:20.00]world\n[01:00]\n");
        assert_eq!(l.lines.len(), 4);
        assert_eq!(l.index_at(4.9), None); // 5.5 s minus the 0.5 s offset
        assert_eq!(l.lines[l.index_at(10.0).unwrap()].1, "hello");
        assert_eq!(l.lines[l.index_at(25.0).unwrap()].1, "world");
    }
}
