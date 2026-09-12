//! The escaping rule every one-terminal-row diagnostic in the importer
//! chain renders untrusted text through (S-4, issues #1037 and #1076).
//!
//! [`ImportError`](crate::ImportError), `pokeemerald-rs`'s own
//! `ImportRomError` and success summary, and that binary's CLI diagnosis all
//! promise one row and all quote something the player named: a ROM path,
//! `$POKEEMERALD_PACK`, a mistyped argument. The rule lives here once so a
//! correction reaches every one of them `(lean-docs)`.

use std::fmt;
use std::path::Path;

/// Text rendered for a one-line diagnostic.
///
/// What a player names is untrusted the same way a cartridge header's bytes
/// are: it may carry a newline or an ESC byte, and the one-terminal-row
/// promise has to survive it. Only what would break the row or steer the
/// terminal is escaped, so ordinary text -- a Windows path included, whose
/// separators are backslashes -- still prints as the literal string a player
/// can copy back into a shell.
///
/// The rendering is therefore ambiguous rather than reversible: text holding
/// the two characters `\` and `n` prints the same as text holding a newline.
/// A diagnostic is read, not parsed, and doubling every separator on the
/// platform where every path has them costs more than the ambiguity does.
pub struct OneLine<'a>(pub &'a str);

impl fmt::Display for OneLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '\n' => f.write_str(r"\n")?,
                '\r' => f.write_str(r"\r")?,
                '\t' => f.write_str(r"\t")?,
                // `is_control` is the Unicode `Cc` category: C0, DEL, and C1.
                // `U+2028`/`U+2029` are line breaks outside it, and the bidi
                // embedding, override, isolate, and mark controls
                // (`Bidi_Control`) can reorder whatever follows on a
                // bidi-aware terminal.
                c if c.is_control()
                    || matches!(
                        c,
                        '\u{2028}'
                            | '\u{2029}'
                            | '\u{061c}'
                            | '\u{200e}'
                            | '\u{200f}'
                            | '\u{202a}'..='\u{202e}'
                            | '\u{2066}'..='\u{2069}'
                    ) =>
                {
                    write!(f, "\\u{{{:x}}}", c as u32)?;
                }
                c => f.write_str(c.encode_utf8(&mut [0u8; 4]))?,
            }
        }
        Ok(())
    }
}

/// A path rendered for a one-line diagnostic, through [`OneLine`].
///
/// A path is bytes rather than text, so it is decoded lossily first; an
/// undecodable byte becomes `U+FFFD`, which is printable and so survives the
/// escaping unchanged.
pub struct OneLinePath<'a>(pub &'a Path);

impl fmt::Display for OneLinePath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", OneLine(&self.0.to_string_lossy()))
    }
}
