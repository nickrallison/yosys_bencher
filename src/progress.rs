use indicatif::{ProgressBar, ProgressStyle};
use std::{
    io::{self, IsTerminal},
    time::Duration,
};

/// Optional shared terminal display. Library callers stay silent by default.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    bar: Option<ProgressBar>,
}

impl Progress {
    pub fn new(label: &str) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(style("{spinner} [{elapsed_precise}] {prefix} {wide_msg}"));
        bar.set_prefix(clean(label, 80));
        if io::stderr().is_terminal() {
            bar.enable_steady_tick(Duration::from_millis(100));
        }
        Self { bar: Some(bar) }
    }

    pub fn start(&self, total: u64) {
        if let Some(bar) = &self.bar {
            bar.set_length(total);
            bar.set_position(0);
            bar.set_style(style(
                "{spinner} [{elapsed_precise}] {pos}/{len} {wide_bar}\n{prefix} {wide_msg}",
            ));
        }
    }

    pub fn context(&self, message: impl AsRef<str>) {
        if let Some(bar) = &self.bar {
            bar.set_prefix(clean(message.as_ref(), 90));
        }
    }

    /// Major phases also produce readable lines when output is redirected.
    pub fn stage(&self, message: impl AsRef<str>) {
        if let Some(bar) = &self.bar {
            let message = clean(message.as_ref(), 150);
            if bar.is_hidden() {
                eprintln!("{}: {message}", bar.prefix());
            }
            bar.set_message(message);
        }
    }

    /// Frequent updates stay on the current line and do not flood redirected logs.
    pub fn detail(&self, message: impl AsRef<str>) {
        if let Some(bar) = &self.bar {
            bar.set_message(clean(message.as_ref(), 150));
        }
    }

    pub fn advance(&self, count: u64) {
        if let Some(bar) = &self.bar {
            bar.inc(count);
        }
    }

    pub fn println(&self, message: impl AsRef<str>) {
        if let Some(bar) = &self.bar {
            let message = clean(message.as_ref(), 240);
            if bar.is_hidden() {
                eprintln!("{message}");
            } else {
                bar.println(message);
            }
        }
    }

    pub fn finish(&self, message: impl AsRef<str>) {
        if let Some(bar) = &self.bar {
            bar.set_prefix("");
            let message = clean(message.as_ref(), 240);
            if bar.is_hidden() {
                eprintln!("{message}");
            }
            bar.set_style(style("[{elapsed_precise}] {msg}"));
            bar.finish_with_message(message);
        }
    }

    pub fn fail(&self, message: impl AsRef<str>) {
        if let Some(bar) = &self.bar
            && !bar.is_finished()
        {
            bar.set_prefix("");
            let message = clean(message.as_ref(), 240);
            if bar.is_hidden() {
                eprintln!("{message}");
            }
            bar.set_style(style("[{elapsed_precise}] {msg}"));
            bar.abandon_with_message(message);
        }
    }
}

fn style(template: &str) -> ProgressStyle {
    ProgressStyle::with_template(template)
        .expect("static progress template")
        .tick_strings(&["-", "\\", "|", "/"])
        .progress_chars("#>-")
}

fn clean(message: &str, limit: usize) -> String {
    let mut chars = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c });
    let mut result: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        result.push_str("...");
    }
    result
}
