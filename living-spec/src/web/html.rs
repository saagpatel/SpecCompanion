use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub(crate) const STYLE: &str = r#"
body{font-family:Georgia,"Iowan Old Style",Palatino,serif;margin:2rem auto;max-width:52rem;color:#1c1c1c;background:#f6f3ee;line-height:1.45}
h1,h2,h3{line-height:1.2}
.counts,.req-id,code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}
.badge{display:inline-block;padding:0 .45rem;border:2px solid;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-weight:700}
.badge-proven{color:#083318;background:#d9f5e3;border-color:#137333;border-style:solid}
.badge-broken{color:#4d0d0d;background:#ffd4d4;border-color:#b42318;border-style:double}
.badge-unproven{color:#222;background:#e4e4e4;border-color:#555;border-style:dotted}
.badge-qualified{color:#3f2a00;background:#ffe4ad;border-color:#8a5a00;border-style:dashed}
.badge-stale{color:#222;background:#e4e4e4;border-color:#555;border-style:dashed}
.badge-unscored{color:#222;background:#eee;border-color:#777;border-style:dashed}
details{margin:.7rem 0;padding:.45rem .7rem;background:#fff;border:1px solid #ddd}
.banner{padding:.7rem .9rem;background:#eee;border:1px dashed #777}
table{border-collapse:collapse;width:100%;background:#fff}
th,td{border:1px solid #ccc;padding:.4rem .55rem;text-align:left;vertical-align:top}
th{background:#efeae2}
caption{text-align:left;font-weight:700;margin-bottom:.4rem}
pre{white-space:pre-wrap;background:#fff;border:1px solid #ccc;padding:.8rem}
"#;

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Proven percentage in tenths, rounded half up. `lines == 0` is 0.
pub(crate) fn percentage_tenths(proven: u64, lines: u64) -> u64 {
    if lines == 0 {
        return 0;
    }
    let proven = u128::from(proven);
    let lines = u128::from(lines);
    let tenths = (proven * 1000 + lines / 2) / lines;
    u64::try_from(tenths).unwrap_or(u64::MAX)
}

pub(crate) fn format_percentage(proven: u64, lines: u64) -> String {
    let tenths = percentage_tenths(proven, lines);
    format!("{}.{}", tenths / 10, tenths % 10)
}

pub(crate) fn percentage(proven: u64, lines: u64) -> f64 {
    (percentage_tenths(proven, lines) as f64) / 10.0
}

pub(crate) fn write_text(path: &Path, text: &str) -> io::Result<()> {
    if path == Path::new("-") || path == Path::new("/dev/stdout") {
        let mut stdout = io::stdout().lock();
        stdout.write_all(text.as_bytes())?;
        stdout.flush()?;
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, text)
}
