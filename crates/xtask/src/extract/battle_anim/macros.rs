//! The assembler macros of `asm/macros/battle_anim_script.inc`.
//!
//! The 48 primitive commands are converted directly (see [`commands`]); every
//! other macro is a wrapper whose body this module expands textually, the way
//! the assembler does: arguments are substituted as text before any
//! expression is evaluated.

use std::collections::HashMap;

use super::commands::{self, Pending};
use super::expr::{is_identifier, Constants, Style};
use super::Fault;

/// How many wrappers may expand into one another before the source is
/// treated as recursive.
const MAX_DEPTH: usize = 8;

#[derive(Debug)]
struct Param {
    name: String,
    required: bool,
    default: Option<String>,
}

#[derive(Debug)]
struct Macro {
    params: Vec<Param>,
    body: Vec<String>,
}

/// The wrapper macros a macro file defines.
#[derive(Debug)]
pub(super) struct Macros {
    definitions: HashMap<String, Macro>,
}

impl Macros {
    /// Reads every non-primitive `.macro` of `text`.
    pub(super) fn parse(file: &str, text: &str) -> Result<Self, Fault> {
        let mut definitions = HashMap::new();
        let mut open: Option<(String, Macro, bool)> = None;
        for (index, raw) in text.lines().enumerate() {
            let fault = |detail: String| Fault::new(file, index + 1, detail);
            let code = strip_comment(raw);
            if let Some(header) = code.strip_prefix(".macro") {
                if open.is_some() {
                    return Err(fault("nested `.macro`".to_owned()));
                }
                let (name, params) = split_word(header);
                let primitive = commands::PRIMITIVES.contains(&name);
                let params = if primitive {
                    Vec::new()
                } else {
                    parse_params(params).map_err(fault)?
                };
                open = Some((
                    name.to_owned(),
                    Macro {
                        params,
                        body: Vec::new(),
                    },
                    primitive,
                ));
            } else if code == ".endm" {
                let (name, definition, primitive) = open
                    .take()
                    .ok_or_else(|| fault("`.endm` without `.macro`".to_owned()))?;
                if !primitive && definitions.insert(name.clone(), definition).is_some() {
                    return Err(fault(format!("macro `{name}` is defined twice")));
                }
            } else if let Some((_, definition, primitive)) = open.as_mut() {
                if !*primitive && !code.is_empty() {
                    definition.body.push(code.to_owned());
                }
            } else if !code.is_empty() {
                return Err(fault(format!("`{code}` is outside a macro")));
            }
        }
        if open.is_some() {
            return Err(Fault::new(file, 0, "unterminated `.macro`".to_owned()));
        }
        Ok(Self { definitions })
    }

    /// Expands the instruction `name args` into primitive commands.
    pub(super) fn expand(
        &self,
        constants: &Constants,
        name: &str,
        args: &str,
        out: &mut Vec<Pending>,
    ) -> Result<(), String> {
        self.dispatch(constants, name, args, 0, out)
    }

    fn dispatch(
        &self,
        constants: &Constants,
        name: &str,
        args: &str,
        depth: usize,
        out: &mut Vec<Pending>,
    ) -> Result<(), String> {
        let args = split_arguments(args)?;
        if let Some(primitive) = commands::convert(name, &args, constants) {
            out.push(primitive?);
            return Ok(());
        }
        let definition = self
            .definitions
            .get(name)
            .ok_or_else(|| format!("unknown command `{name}`"))?;
        if depth >= MAX_DEPTH {
            return Err(format!("macro `{name}` expands too deeply"));
        }
        let bound = bind(name, definition, &args)?;
        let mut branches: Vec<Branch> = Vec::new();
        for line in &definition.body {
            let line = substitute(line, &bound).map_err(|e| format!("macro `{name}`: {e}"))?;
            let (word, rest) = split_word(&line);
            if apply_conditional(constants, word, rest, &mut branches)
                .map_err(|e| format!("macro `{name}`: {e}"))?
            {
                continue;
            }
            if branches.last().is_some_and(|b| !b.active) {
                continue;
            }
            match word {
                ".error" => return Err(format!("macro `{name}` raised an error: {rest}")),
                ".warning" => {}
                _ => self.dispatch(constants, word, rest, depth + 1, out)?,
            }
        }
        if branches.is_empty() {
            Ok(())
        } else {
            Err(format!("macro `{name}` has an unterminated `.if`"))
        }
    }
}

/// One open `.if` in a macro body.
struct Branch {
    parent_active: bool,
    active: bool,
    taken: bool,
}

/// Handles `.if`-family directives; returns whether `word` was one.
fn apply_conditional(
    constants: &Constants,
    word: &str,
    rest: &str,
    branches: &mut Vec<Branch>,
) -> Result<bool, String> {
    let parent_active = branches.last().is_none_or(|b| b.active);
    let condition = match word {
        ".ifb" => rest.is_empty(),
        ".ifnb" => !rest.is_empty(),
        ".if" if parent_active => condition(constants, rest)?,
        ".if" => false,
        ".else" => {
            let branch = branches.last_mut().ok_or("`.else` without `.if`")?;
            branch.active = branch.parent_active && !branch.taken;
            branch.taken = true;
            return Ok(true);
        }
        ".endif" => {
            branches.pop().ok_or("`.endif` without `.if`")?;
            return Ok(true);
        }
        _ => return Ok(false),
    };
    branches.push(Branch {
        parent_active,
        active: parent_active && condition,
        taken: parent_active && condition,
    });
    Ok(true)
}

/// Evaluates `a == b || c == d`.
fn condition(constants: &Constants, text: &str) -> Result<bool, String> {
    for alternative in text.split("||") {
        let (left, right) = alternative
            .split_once("==")
            .ok_or_else(|| format!("unsupported condition `{text}`"))?;
        if constants.eval(left, Style::Gas)? == constants.eval(right, Style::Gas)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn bind(
    name: &str,
    definition: &Macro,
    args: &[String],
) -> Result<HashMap<String, String>, String> {
    let mut bound: HashMap<String, String> = HashMap::new();
    let mut positional = 0;
    for arg in args {
        let named = arg.split_once('=').and_then(|(key, value)| {
            let key = key.trim();
            definition
                .params
                .iter()
                .any(|p| p.name == key)
                .then(|| (key, value.trim()))
        });
        let (key, value) = if let Some(named) = named {
            named
        } else {
            while definition
                .params
                .get(positional)
                .is_some_and(|p| bound.contains_key(&p.name))
            {
                positional += 1;
            }
            let param = definition
                .params
                .get(positional)
                .ok_or_else(|| format!("macro `{name}` got too many operands"))?;
            (param.name.as_str(), arg.as_str())
        };
        if bound.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!("macro `{name}` binds `{key}` twice"));
        }
    }
    for param in &definition.params {
        if bound.get(&param.name).is_some_and(|v| !v.is_empty()) {
            continue;
        }
        match (&param.default, param.required) {
            (Some(default), _) => bound.insert(param.name.clone(), default.clone()),
            (None, true) => return Err(format!("macro `{name}` lacks `{}`", param.name)),
            (None, false) => bound.insert(param.name.clone(), String::new()),
        };
    }
    Ok(bound)
}

fn substitute(line: &str, bound: &HashMap<String, String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let value = bound
            .get(&after[..end])
            .ok_or_else(|| format!("`\\{}` is not a parameter", &after[..end]))?;
        out.push_str(value);
        rest = &after[end..];
    }
    out.push_str(rest);
    Ok(out.trim().to_owned())
}

fn parse_params(text: &str) -> Result<Vec<Param>, String> {
    split_arguments(text)?
        .iter()
        .map(|entry| {
            let (declaration, default) = match entry.split_once('=') {
                Some((declaration, default)) => {
                    (declaration.trim(), Some(default.trim().to_owned()))
                }
                None => (entry.as_str(), None),
            };
            let (name, qualifier) = declaration.split_once(':').unwrap_or((declaration, ""));
            if !is_identifier(name) || !matches!(qualifier, "" | "req") {
                return Err(format!("unsupported macro parameter `{entry}`"));
            }
            Ok(Param {
                name: name.to_owned(),
                required: qualifier == "req",
                default,
            })
        })
        .collect()
}

/// Splits on commas outside parentheses; empty text has no operands.
pub(super) fn split_arguments(text: &str) -> Result<Vec<String>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let (mut parts, mut depth, mut start) = (Vec::new(), 0_usize, 0);
    for (index, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1).ok_or("unbalanced `)`")?,
            ',' if depth == 0 => {
                parts.push(text[start..index].trim().to_owned());
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("unbalanced `(`".to_owned());
    }
    parts.push(text[start..].trim().to_owned());
    Ok(parts)
}

/// Drops an `@` comment and surrounding whitespace.
pub(super) fn strip_comment(line: &str) -> &str {
    line.split('@').next().unwrap_or_default().trim()
}

/// Splits the first whitespace-delimited word from the rest.
pub(super) fn split_word(line: &str) -> (&str, &str) {
    let line = line.trim();
    line.split_once(char::is_whitespace)
        .map_or((line, ""), |(word, rest)| (word, rest.trim()))
}
