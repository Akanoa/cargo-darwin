use std::fmt::{Display, Write};
use std::hash::Hash;
use std::iter::zip;
use std::ops::Range;

use colored::{ColoredString, Colorize};
use imara_diff::intern::{InternedInput, Interner, Token};
use imara_diff::Sink;
use text_diff::Difference;

/// A [`Sink`](crate::sink::Sink) that creates a textual diff
/// in the format typically output by git or gnu-diff if the `-u` option is used
pub struct UnifiedColorDiff<'a, W, T>
where
    W: Write,
    T: Hash + Eq + Display,
{
    before: &'a [Token],
    after: &'a [Token],
    interner: &'a Interner<T>,

    pos: u32,
    before_hunk_start: u32,
    after_hunk_start: u32,
    before_hunk_len: u32,
    after_hunk_len: u32,

    buffer: String,
    dst: W,
}

impl<'a, T> UnifiedColorDiff<'a, String, T>
where
    T: Hash + Eq + Display,
{
    /// Create a new `UnifiedColorDiff` for the given `input`,
    /// that will return a [`String`](String).
    pub fn new(input: &'a InternedInput<T>) -> Self {
        Self {
            before_hunk_start: 0,
            after_hunk_start: 0,
            before_hunk_len: 0,
            after_hunk_len: 0,
            buffer: String::with_capacity(8),
            dst: String::new(),
            interner: &input.interner,
            before: &input.before,
            after: &input.after,
            pos: 0,
        }
    }
}

impl<'a, W, T> UnifiedColorDiff<'a, W, T>
where
    W: Write,
    T: Hash + Eq + Display,
{
    fn print_tokens(&mut self, tokens: &[Token], prefix: &str) {
        let prefix = match prefix {
            "+" => "+".green(),
            "-" => "-".red(),
            _ => prefix.white(),
        };

        for &token in tokens {
            writeln!(&mut self.buffer, "{}{}", prefix, self.interner[token]).unwrap();
        }
    }

    fn flush(&mut self) {
        if self.before_hunk_len == 0 && self.after_hunk_len == 0 {
            return;
        }

        let end = (self.pos + 3).min(self.before.len() as u32);
        self.update_pos(end, end);

        writeln!(
            &mut self.dst,
            "@@ -{},{} +{},{} @@",
            self.before_hunk_start + 1,
            self.before_hunk_len,
            self.after_hunk_start + 1,
            self.after_hunk_len,
        )
        .unwrap();
        write!(&mut self.dst, "{}", &self.buffer).unwrap();
        self.buffer.clear();
        self.before_hunk_len = 0;
        self.after_hunk_len = 0
    }

    fn update_pos(&mut self, print_to: u32, move_to: u32) {
        self.print_tokens(&self.before[self.pos as usize..print_to as usize], " ");
        let len = print_to - self.pos;
        self.pos = move_to;
        self.before_hunk_len += len;
        self.after_hunk_len += len;
    }
}

impl<W, T> Sink for UnifiedColorDiff<'_, W, T>
where
    W: Write,
    T: Hash + Eq + Display,
{
    type Out = W;

    fn process_change(&mut self, before: Range<u32>, after: Range<u32>) {
        if before.start - self.pos > 6 {
            self.flush();
            self.pos = before.start - 3;
            self.before_hunk_start = self.pos;
            self.after_hunk_start = after.start - 3;
        }
        self.update_pos(before.start, before.end);
        self.before_hunk_len += before.end - before.start;
        self.after_hunk_len += after.end - after.start;

        let before = &self.before[before.start as usize..before.end as usize];
        let after = &self.after[after.start as usize..after.end as usize];

        fn format_diff_old(diffs: Vec<Difference>) -> String {
            diffs
                .iter()
                .filter(|diff| {
                    if let Difference::Add(..) = diff {
                        false
                    } else {
                        true
                    }
                })
                .map(|diff| match diff {
                    Difference::Same(x) => x.normal(),
                    Difference::Add(x) => x.green(),
                    Difference::Rem(x) => x.red(),
                })
                .collect::<Vec<ColoredString>>()
                .iter()
                .fold("".to_string(), |mut acc, x| {
                    acc = format!("{acc}{x}");
                    acc
                })
        }

        fn format_diff_new(diffs: Vec<Difference>) -> String {
            diffs
                .iter()
                .filter(|diff| {
                    if let Difference::Add(..) = diff {
                        false
                    } else {
                        true
                    }
                })
                .map(|diff| match diff {
                    Difference::Same(x) => x.normal(),
                    Difference::Add(x) => x.red(),
                    Difference::Rem(x) => x.green(),
                })
                .collect::<Vec<ColoredString>>()
                .iter()
                .fold("".to_string(), |mut acc, x| {
                    acc = format!("{acc}{x}");
                    acc
                })
        }

        for (before_token, after_token) in zip(before, after) {
            let old = format!("{}", self.interner[*before_token]);
            let new = format!("{}", self.interner[*after_token]);
            let (_, diff) = text_diff::diff(&old, &new, "");
            writeln!(&mut self.buffer, "{}{}", "-".red(), format_diff_old(diff)).unwrap();
            let (_, diff) = text_diff::diff(&new, &old, "");
            writeln!(&mut self.buffer, "{}{}", "+".green(), format_diff_new(diff)).unwrap();
        }
    }

    fn finish(mut self) -> Self::Out {
        self.flush();
        self.dst
    }
}
