//! Parser for `sound/cry_tables.inc`: the normal and reverse `cry` record lists.

use super::LineError;

const NORMAL_LABEL: &str = "gCryTable";
const REVERSE_LABEL: &str = "gCryTable_Reverse";
const NORMAL_MACRO: &str = "cry";
const REVERSE_MACRO: &str = "cry_reverse";

/// The two symbol lists in file order.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RawCryTables {
    pub(super) normal: Vec<String>,
    pub(super) reverse: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Table {
    Normal,
    Reverse,
}

/// Strips a trailing `@` comment and surrounding whitespace.
pub(super) fn code_of(line: &str) -> &str {
    line.split('@').next().unwrap_or_default().trim()
}

/// Whether `symbol` is a plain `Cry_*` identifier.
pub(super) fn is_cry_symbol(symbol: &str) -> bool {
    symbol
        .strip_prefix("Cry_")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Parses `cry_tables.inc`.
///
/// Only blank lines, `.align 2`, the two table labels, and one-operand `cry`/`cry_reverse`
/// records inside the matching table are accepted. Each table must appear exactly once, normal
/// first, and the two lists must name the same symbols at every position (`cry_reverse` differs
/// from `cry` only in the tone type's reverse flag, `asm/macros/music_voice.inc`).
pub(super) fn parse_cry_tables(text: &str) -> Result<RawCryTables, LineError> {
    let mut normal: Option<Vec<String>> = None;
    let mut reverse: Option<Vec<String>> = None;
    let mut current: Option<Table> = None;

    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let code = code_of(line);
        if code.is_empty() || code == ".align 2" {
            continue;
        }
        if let Some(label) = code.strip_suffix("::") {
            let table = match label {
                NORMAL_LABEL => Table::Normal,
                REVERSE_LABEL => Table::Reverse,
                other => {
                    return Err(LineError::new(
                        number,
                        format!("unexpected label `{other}`"),
                    ))
                }
            };
            if table == Table::Reverse && normal.is_none() {
                return Err(LineError::new(
                    number,
                    "reverse table precedes the normal table",
                ));
            }
            let slot = if table == Table::Normal {
                &mut normal
            } else {
                &mut reverse
            };
            if slot.is_some() {
                return Err(LineError::new(number, format!("duplicate table `{label}`")));
            }
            *slot = Some(Vec::new());
            current = Some(table);
            continue;
        }

        let mut fields = code.split_whitespace();
        let macro_name = fields.next().unwrap_or_default();
        let (expected, list) = match current {
            Some(Table::Normal) => (NORMAL_MACRO, normal.as_mut()),
            Some(Table::Reverse) => (REVERSE_MACRO, reverse.as_mut()),
            None => {
                return Err(LineError::new(
                    number,
                    format!("`{code}` outside a cry table"),
                ));
            }
        };
        if macro_name != expected {
            return Err(LineError::new(
                number,
                format!("expected a `{expected}` record, found `{code}`"),
            ));
        }
        let (Some(symbol), None) = (fields.next(), fields.next()) else {
            return Err(LineError::new(
                number,
                format!("`{expected}` takes exactly one sample symbol"),
            ));
        };
        if !is_cry_symbol(symbol) {
            return Err(LineError::new(
                number,
                format!("`{symbol}` is not a `Cry_*` symbol"),
            ));
        }
        let list = list.expect("a current table always has a list");
        if list.iter().any(|seen| seen == symbol) {
            return Err(LineError::new(
                number,
                format!("duplicate symbol `{symbol}`"),
            ));
        }
        list.push(symbol.to_owned());
    }

    let normal =
        normal.ok_or_else(|| LineError::new(0, format!("missing table `{NORMAL_LABEL}`")))?;
    let reverse =
        reverse.ok_or_else(|| LineError::new(0, format!("missing table `{REVERSE_LABEL}`")))?;
    check_pairing(&normal, &reverse)?;
    Ok(RawCryTables { normal, reverse })
}

/// Requires equal-length lists naming the same symbol at every position.
fn check_pairing(normal: &[String], reverse: &[String]) -> Result<(), LineError> {
    if normal.len() != reverse.len() {
        return Err(LineError::new(
            0,
            format!(
                "normal table has {} records but reverse table has {}",
                normal.len(),
                reverse.len()
            ),
        ));
    }
    if let Some(position) = normal.iter().zip(reverse).position(|(n, r)| n != r) {
        return Err(LineError::new(
            0,
            format!(
                "position {position} names `{}` in the normal table but `{}` in the reverse table",
                normal[position], reverse[position]
            ),
        ));
    }
    Ok(())
}
