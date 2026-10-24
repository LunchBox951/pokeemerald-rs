//! The script file: selector tables, labels, and the expanded command stream.

use std::collections::HashMap;

use assets::AnimationTable;

use super::commands::Pending;
use super::expr::{is_identifier, Constants};
use super::macros::{split_word, strip_comment, Macros};
use super::Fault;

/// The upstream symbol of each selector table, in [`AnimationTable::ALL`] order.
const TABLE_SYMBOLS: [&str; 4] = [
    "gBattleAnims_Moves",
    "gBattleAnims_StatusConditions",
    "gBattleAnims_General",
    "gBattleAnims_Special",
];
/// The move table's extra last entry, which upstream says "cannot be reached".
const MOVE_TABLE_SENTINEL: &str = "Move_COUNT";

/// A failure located by its one-based script line.
pub(super) type Located<T> = Result<T, (usize, String)>;

/// One expanded primitive command and the script line it came from.
#[derive(Debug)]
pub(super) struct Item {
    pub(super) pending: Pending,
    pub(super) line: usize,
}

#[derive(Debug)]
struct Label {
    name: String,
    /// Position in the command stream of the first command at or after it.
    position: usize,
    line: usize,
    /// The label names `.2byte`/`.4byte` data, not commands.
    data: bool,
}

/// A parsed script.
#[derive(Debug)]
pub(super) struct Script {
    stream: Vec<Item>,
    labels: Vec<Label>,
    by_name: HashMap<String, usize>,
    /// Stream positions where data directives sit between commands.
    data_boundaries: Vec<usize>,
    tables: [Vec<(String, usize)>; 4],
}

struct Conditional {
    active: bool,
    parent_active: bool,
}

impl Script {
    /// Parses `text`, expanding wrapper macros with `macros`.
    pub(super) fn parse(
        file: &str,
        text: &str,
        constants: &Constants,
        macros: &Macros,
    ) -> Result<Self, Fault> {
        let mut script = Self {
            stream: Vec::new(),
            labels: Vec::new(),
            by_name: HashMap::new(),
            data_boundaries: Vec::new(),
            tables: Default::default(),
        };
        let mut open_table: Option<usize> = None;
        let mut conditionals: Vec<Conditional> = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let fault = |detail: String| Fault::new(file, line, detail);
            let mut code = strip_comment(raw);
            if code.starts_with('#') {
                continue;
            }
            while let Some((label, rest)) = split_label(code) {
                if conditionals.last().is_none_or(|c| c.active) {
                    open_table = TABLE_SYMBOLS.iter().position(|t| *t == label);
                    script.add_label(label, line).map_err(fault)?;
                }
                code = rest;
            }
            let (word, operands) = split_word(code);
            if word.is_empty() {
                continue;
            }
            match word {
                ".ifdef" | ".ifndef" => {
                    let parent_active = conditionals.last().is_none_or(|c| c.active);
                    let defined = constants.defines(operands);
                    conditionals.push(Conditional {
                        active: parent_active && (defined == (word == ".ifdef")),
                        parent_active,
                    });
                    continue;
                }
                ".else" => {
                    let top = conditionals
                        .last_mut()
                        .ok_or_else(|| fault("`.else` without `.ifdef`".to_owned()))?;
                    top.active = top.parent_active && !top.active;
                    continue;
                }
                ".endif" => {
                    conditionals
                        .pop()
                        .ok_or_else(|| fault("`.endif` without `.ifdef`".to_owned()))?;
                    continue;
                }
                _ => {}
            }
            if conditionals.last().is_some_and(|c| !c.active) {
                continue;
            }
            match word {
                ".4byte" => {
                    let table = open_table
                        .ok_or_else(|| fault("`.4byte` outside a selector table".to_owned()))?;
                    if !is_identifier(operands) {
                        return Err(fault(format!("`{operands}` is not a script label")));
                    }
                    script.tables[table].push((operands.to_owned(), line));
                    script.mark_data();
                }
                ".2byte" => script.mark_data(),
                ".align" | ".section" | ".include" => {}
                _ if word.starts_with('.') => {
                    return Err(fault(format!("unsupported directive `{word}`")));
                }
                _ => {
                    if open_table.is_some() {
                        return Err(fault(format!("`{word}` inside a selector table")));
                    }
                    if script.labels.is_empty() {
                        return Err(fault(format!("`{word}` before any label")));
                    }
                    let mut expanded = Vec::new();
                    macros
                        .expand(constants, word, operands, &mut expanded)
                        .map_err(&fault)?;
                    script
                        .stream
                        .extend(expanded.into_iter().map(|pending| Item { pending, line }));
                }
            }
        }
        if !conditionals.is_empty() {
            return Err(Fault::new(file, 0, "unterminated `.ifdef`".to_owned()));
        }
        script
            .check_tables()
            .map_err(|(line, d)| Fault::new(file, line, d))?;
        Ok(script)
    }

    fn add_label(&mut self, name: &str, line: usize) -> Result<(), String> {
        if self
            .by_name
            .insert(name.to_owned(), self.labels.len())
            .is_some()
        {
            return Err(format!("label `{name}` is defined twice"));
        }
        self.labels.push(Label {
            name: name.to_owned(),
            position: self.stream.len(),
            line,
            data: false,
        });
        Ok(())
    }

    /// Records a data directive: every label stacked immediately before it
    /// names data, and no program may run across it.
    fn mark_data(&mut self) {
        let position = self.stream.len();
        self.data_boundaries.push(position);
        for label in self.labels.iter_mut().filter(|l| l.position == position) {
            label.data = true;
        }
    }

    /// Requires each table to hold exactly its selectors, and drops the move
    /// table's unreachable sentinel.
    fn check_tables(&mut self) -> Located<()> {
        for (table, entries) in AnimationTable::ALL.into_iter().zip(&mut self.tables) {
            if table == AnimationTable::Moves {
                match entries.pop() {
                    Some((label, _)) if label == MOVE_TABLE_SENTINEL => {}
                    other => {
                        let line = other.map_or(0, |(_, line)| line);
                        return Err((
                            line,
                            format!("the move table must end with `{MOVE_TABLE_SENTINEL}`"),
                        ));
                    }
                }
            }
            if entries.len() != usize::from(table.len()) {
                return Err((
                    entries.first().map_or(0, |(_, line)| *line),
                    format!(
                        "the {table} table lists {} selectors: expected {}",
                        entries.len(),
                        table.len()
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Requires every control-flow target in the file to be a command label.
    pub(super) fn check_targets(&self) -> Located<()> {
        for item in &self.stream {
            for target in &item.pending.targets {
                self.label_index(target, item.line)?;
            }
        }
        Ok(())
    }

    /// The selector entries of `table`, as `(label, line)` in slot order.
    pub(super) fn table(&self, table: AnimationTable) -> &[(String, usize)] {
        let position = AnimationTable::ALL.iter().position(|t| *t == table);
        &self.tables[position.unwrap_or_default()]
    }

    /// The label that `name` resolves to, which must start a command.
    pub(super) fn label_index(&self, name: &str, line: usize) -> Located<usize> {
        let index = *self
            .by_name
            .get(name)
            .ok_or_else(|| (line, format!("label `{name}` is not defined")))?;
        if self.labels[index].data {
            return Err((line, format!("label `{name}` names data, not commands")));
        }
        if self.labels[index].position >= self.stream.len() {
            return Err((line, format!("label `{name}` has no commands")));
        }
        Ok(index)
    }

    /// A label's name and line.
    pub(super) fn label(&self, index: usize) -> (&str, usize) {
        let label = &self.labels[index];
        (&label.name, label.line)
    }

    /// The commands from a label through the first terminator, wherever it falls.
    pub(super) fn program(&self, index: usize) -> Located<&[Item]> {
        let label = &self.labels[index];
        let tail = &self.stream[label.position..];
        let end = tail
            .iter()
            .position(|item| item.pending.terminates())
            .ok_or_else(|| {
                (
                    label.line,
                    format!("program `{}` never terminates", label.name),
                )
            })?;
        if self
            .data_boundaries
            .iter()
            .any(|b| *b > label.position && *b <= label.position + end)
        {
            return Err((
                label.line,
                format!("program `{}` runs into data", label.name),
            ));
        }
        Ok(&tail[..=end])
    }
}

/// Splits a leading `name:` or `name::` label from `code`.
fn split_label(code: &str) -> Option<(&str, &str)> {
    let (word, rest) = code.split_once(':')?;
    let word = word.trim();
    is_identifier(word).then(|| (word, rest.trim_start_matches(':').trim()))
}
