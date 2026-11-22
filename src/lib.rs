mod marks;

use std::fmt;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

pub const FALLBACK: &str = "en";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Number(String),
    Key(String),
    Nested(Box<Msg>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    pub key: String,
    pub args: Vec<(String, Value)>,
}

impl Msg {
    pub fn new(key: &str) -> Self {
        Msg { key: key.to_string(), args: Vec::new() }
    }

    pub fn arg(mut self, name: &str, value: impl fmt::Display) -> Self {
        self.args.push((name.to_string(), Value::Text(value.to_string())));
        self
    }

    pub fn num(mut self, name: &str, value: impl fmt::Display) -> Self {
        self.args.push((name.to_string(), Value::Number(value.to_string())));
        self
    }

    pub fn sub(mut self, name: &str, key: &str) -> Self {
        self.args.push((name.to_string(), Value::Key(key.to_string())));
        self
    }

    pub fn args<'a>(mut self, args: impl IntoIterator<Item = (&'a str, String)>) -> Self {
        for (name, value) in args {
            self.args.push((name.to_string(), Value::Text(value)));
        }
        self
    }

    pub fn nest(mut self, name: &str, inner: Msg) -> Self {
        self.args.push((name.to_string(), Value::Nested(Box::new(inner))));
        self
    }
}

fn quoted(value: &str) -> String {
    let clean = printable(value);
    let plain = !clean.is_empty() && !clean.chars().any(|c| c.is_whitespace() || matches!(c, '=' | '"' | '(' | ')' | '\\'));
    if plain { clean } else { format!("\"{}\"", clean.replace('\\', "\\\\").replace('"', "\\\"")) }
}

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", printable(&self.key))?;
        for (name, value) in &self.args {
            let name = printable(name);
            match value {
                Value::Text(v) | Value::Number(v) | Value::Key(v) => write!(f, " {name}={}", quoted(v))?,
                Value::Nested(inner) => write!(f, " {name}=({inner})")?,
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fail(pub Msg);

impl Fail {
    pub fn new(key: &str) -> Self {
        Fail(Msg::new(key))
    }

    pub fn arg(self, name: &str, value: impl fmt::Display) -> Self {
        Fail(self.0.arg(name, value))
    }

    pub fn num(self, name: &str, value: impl fmt::Display) -> Self {
        Fail(self.0.num(name, value))
    }

    pub fn sub(self, name: &str, key: &str) -> Self {
        Fail(self.0.sub(name, key))
    }

    pub fn args<'a>(self, args: impl IntoIterator<Item = (&'a str, String)>) -> Self {
        Fail(self.0.args(args))
    }

    pub fn nest(self, name: &str, inner: Msg) -> Self {
        Fail(self.0.nest(name, inner))
    }
}

impl From<Msg> for Fail {
    fn from(m: Msg) -> Self {
        Fail(m)
    }
}

impl fmt::Display for Fail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Fail {}

pub fn as_msg(error: &(dyn std::error::Error + 'static), fallback_key: &str) -> Msg {
    match error.downcast_ref::<Fail>() {
        Some(Fail(m)) => m.clone(),
        None => Msg::new(fallback_key).arg("detail", error),
    }
}

pub fn language_of(locale: &str) -> Option<String> {
    let lang = locale.split(['_', '.', '@', '-']).next().unwrap_or("").to_ascii_lowercase();
    let plausible = (2..=8).contains(&lang.len()) && lang.bytes().all(|b| b.is_ascii_lowercase());
    (plausible && lang != "posix").then_some(lang)
}

pub fn untranslated(locale: &str) -> bool {
    let name = locale.split(['.', '@']).next().unwrap_or("");
    name == "C" || name == "POSIX"
}

pub fn languages_from(variable: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let set = |name: &str| variable(name).filter(|value| !value.is_empty());
    let Some(base) = ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().find_map(set).as_deref().and_then(language_of) else {
        return Vec::new();
    };
    let Some(preferred) = set("LANGUAGE") else { return vec![base] };
    let mut out: Vec<String> = Vec::new();
    for language in preferred.split(':').take_while(|item| !untranslated(item)).filter_map(language_of) {
        if !out.contains(&language) {
            out.push(language);
        }
    }
    out
}

pub fn requested_languages() -> Vec<String> {
    languages_from(|name| std::env::var(name).ok())
}

fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00ad}'
                | '\u{034f}'
                | '\u{061c}'
                | '\u{115f}'..='\u{1160}'
                | '\u{17b4}'..='\u{17b5}'
                | '\u{180b}'..='\u{180f}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{2800}'
                | '\u{3164}'
                | '\u{e000}'..='\u{f8ff}'
                | '\u{fdd0}'..='\u{fdef}'
                | '\u{fe00}'..='\u{fe0f}'
                | '\u{feff}'
                | '\u{ffa0}'
                | '\u{fff0}'..='\u{fffb}'
                | '\u{13430}'..='\u{1343f}'
                | '\u{16fe4}'
                | '\u{1bca0}'..='\u{1bca3}'
                | '\u{1d159}'
                | '\u{1d173}'..='\u{1d17a}'
                | '\u{e0000}'..='\u{e0fff}'
                | '\u{f0000}'..='\u{10ffff}'
        )
        || (c as u32) & 0xfffe == 0xfffe
}

fn prepended(c: char) -> bool {
    matches!(c, '\u{0600}'..='\u{0605}' | '\u{06dd}' | '\u{070f}' | '\u{0890}'..='\u{0891}' | '\u{08e2}' | '\u{110bd}' | '\u{110cd}')
}

fn wide_space(c: char) -> bool {
    matches!(c, '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

fn listed(table: &[(u32, u32)], c: char) -> bool {
    let point = c as u32;
    let side = |&(first, last): &(u32, u32)| {
        if last < point {
            std::cmp::Ordering::Less
        } else if first > point {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    };
    table.binary_search_by(side).is_ok()
}

fn stacker(c: char) -> bool {
    listed(&marks::STACKERS, c) && !matches!(c, '\u{0900}'..='\u{0eff}' | '\u{103a}')
}

fn stacking(c: char) -> bool {
    c >= '\u{0300}' && !hidden(c) && (listed(&marks::MARKS, c) || matches!(c, '\u{1161}'..='\u{11ff}' | '\u{d7b0}'..='\u{d7ff}'))
}

fn joined_script(c: char) -> bool {
    matches!(
        c,
        '\u{0600}'..='\u{0dff}'
            | '\u{1000}'..='\u{109f}'
            | '\u{1780}'..='\u{17ff}'
            | '\u{1800}'..='\u{18af}'
            | '\u{1b00}'..='\u{1b7f}'
            | '\u{a980}'..='\u{a9df}'
            | '\u{fb50}'..='\u{fdcf}'
            | '\u{fdf0}'..='\u{fdff}'
            | '\u{fe70}'..='\u{fefe}'
            | '\u{11000}'..='\u{11fff}'
    )
}

fn pictured(c: char) -> bool {
    c >= '\u{2190}' && !c.is_alphanumeric() && !c.is_whitespace() && !hidden(c) && !wide_space(c)
}

fn ideograph(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{323af}')
}

fn joins(before: Option<char>, after: Option<char>) -> bool {
    let joinable = |side: Option<char>| side.is_some_and(|c| joined_script(c) || pictured(c));
    joinable(before) && joinable(after)
}

fn styles(c: char, before: Option<char>, after: Option<char>) -> bool {
    let Some(base) = before else { return false };
    match c {
        '\u{fe0e}' | '\u{fe0f}' => {
            pictured(base)
                || matches!(base, '\u{00a9}' | '\u{00ae}' | '\u{203c}' | '\u{2049}' | '\u{2122}' | '\u{2139}' | '\u{24b6}'..='\u{24e9}' | '\u{1f130}'..='\u{1f189}')
                || (matches!(base, '0'..='9' | '#' | '*') && after == Some('\u{20e3}'))
        }
        '\u{180b}'..='\u{180d}' | '\u{180f}' => ('\u{1820}'..='\u{18aa}').contains(&base),
        '\u{e0100}'..='\u{e01ef}' => ideograph(base),
        _ => false,
    }
}

const MAX_STACK: usize = 8;
const MAX_JOINED: usize = 4;

pub fn printable_lines(text: &str) -> String {
    text.split('\n').map(printable).collect::<Vec<_>>().join("\n")
}

pub fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut stack, mut chain, mut base, mut joined, mut styled) = (0, 0, None, false, false);
    let mut rest = text.chars().peekable();
    while let Some(c) = rest.next() {
        let after = rest.peek().copied();
        if stacking(c) {
            stack += 1;
            chain += usize::from(stacker(c));
            let kept = stack <= MAX_STACK && chain <= MAX_JOINED;
            if kept {
                out.push(c);
            }
            joined |= kept && stacker(c);
            continue;
        }
        if matches!(c, '\u{200c}' | '\u{200d}') && joins(base, after) && !out.ends_with(['\u{200c}', '\u{200d}']) {
            out.push(c);
            continue;
        }
        if !styled && styles(c, base, after) {
            out.push(c);
            styled = true;
            continue;
        }
        if prepended(c) {
            out.push(c);
            continue;
        }
        let shown = match c {
            c if hidden(c) => char::REPLACEMENT_CHARACTER,
            c if wide_space(c) => ' ',
            c => c,
        };
        if !(joined && shown == c && !c.is_ascii() && !c.is_whitespace()) {
            (stack, chain) = (0, 0);
        }
        (joined, styled) = (false, false);
        out.push(shown);
        base = (shown == c).then_some(c);
    }
    out
}

const EXACT_DIGITS: usize = 15;

fn whole_part(number: &str) -> Option<&str> {
    let digits = number.strip_prefix('-').unwrap_or(number);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    let numeric = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    (numeric(whole) && numeric(fraction)).then_some(whole)
}

fn plain_number(number: &str) -> bool {
    whole_part(number).is_some()
}

fn countable(number: &str) -> bool {
    whole_part(number).is_some_and(|whole| (whole == "0" || !whole.starts_with('0')) && number.trim_start_matches('-').len() <= EXACT_DIGITS)
}

const MAX_NESTING: usize = 4;

type Bundle = FluentBundle<FluentResource>;

fn bundle(lang: &str, sources: &[&str]) -> Option<Bundle> {
    if sources.is_empty() {
        return None;
    }
    let id: LanguageIdentifier = lang.parse().ok()?;
    let mut b = FluentBundle::new_concurrent(vec![id]);
    b.set_use_isolating(false);
    for source in sources {
        b.add_resource(FluentResource::try_new(source.to_string()).ok()?).ok()?;
    }
    Some(b)
}

pub struct Catalog {
    primary: Option<Bundle>,
    fallback: Option<Bundle>,
    language: String,
}

impl Catalog {
    pub fn new(sources: &[(&str, &str)]) -> Self {
        Self::for_languages(sources, &requested_languages())
    }

    pub fn for_languages(sources: &[(&str, &str)], wanted: &[String]) -> Self {
        let load = |lang: &str| {
            let parts: Vec<&str> = sources.iter().filter(|(l, _)| *l == lang).map(|(_, s)| *s).collect();
            bundle(lang, &parts)
        };
        let language = wanted.iter().find(|w| sources.iter().any(|(l, _)| l == w)).cloned().unwrap_or_else(|| FALLBACK.to_string());
        Catalog { primary: load(&language), fallback: load(FALLBACK), language }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn is_loaded(&self) -> bool {
        self.primary.is_some()
    }

    pub fn has(&self, key: &str) -> bool {
        self.primary.as_ref().is_some_and(|b| b.get_message(key).is_some_and(|m| m.value().is_some()))
    }

    fn compose(&self, msg: &Msg, outer: &[(String, FluentValue<'static>)], depth: usize, problems: &mut Vec<String>) -> String {
        let mut values: Vec<(String, FluentValue<'static>)> = outer.to_vec();
        let put = |values: &mut Vec<(String, FluentValue<'static>)>, name: &str, value: FluentValue<'static>| match values
            .iter_mut()
            .find(|(known, _)| known == name)
        {
            Some(slot) => slot.1 = value,
            None => values.push((name.to_string(), value)),
        };
        for (name, value) in &msg.args {
            match value {
                Value::Text(text) => put(&mut values, name, FluentValue::from(printable(text))),
                Value::Number(number) if countable(number) => put(&mut values, name, FluentValue::try_number(number).into_owned()),
                Value::Number(number) => {
                    if !plain_number(number) {
                        problems.push(format!("{}: {name} is not a number", printable(&msg.key)));
                    }
                    put(&mut values, name, FluentValue::from(printable(number)));
                }
                Value::Key(_) | Value::Nested(_) => {}
            }
        }
        let scalars = values.clone();
        for (name, value) in &msg.args {
            let text = match value {
                Value::Key(key) => self.format(key, &scalars, problems),
                Value::Nested(inner) if depth < MAX_NESTING => self.compose(inner, &[], depth + 1, problems),
                Value::Nested(inner) => {
                    problems.push(format!("{}: nested too deep", printable(&inner.key)));
                    printable(&inner.key)
                }
                Value::Text(_) | Value::Number(_) => continue,
            };
            put(&mut values, name, FluentValue::from(text));
        }
        if !self.knows(&msg.key) {
            problems.push(format!("{}: no such message", printable(&msg.key)));
            return msg.to_string();
        }
        self.format(&msg.key, &values, problems)
    }

    fn knows(&self, key: &str) -> bool {
        [&self.primary, &self.fallback].into_iter().flatten().any(|b| b.get_message(key).is_some_and(|m| m.value().is_some()))
    }

    pub fn try_render(&self, msg: &Msg) -> Result<String, Vec<String>> {
        let mut problems = Vec::new();
        let text = printable_lines(&self.compose(msg, &[], 0, &mut problems));
        if problems.is_empty() { Ok(text) } else { Err(problems) }
    }

    pub fn render(&self, msg: &Msg) -> String {
        printable_lines(&self.compose(msg, &[], 0, &mut Vec::new()))
    }

    pub fn text(&self, key: &str) -> String {
        self.render(&Msg::new(key))
    }

    fn format(&self, key: &str, values: &[(String, FluentValue<'static>)], problems: &mut Vec<String>) -> String {
        let mut args = FluentArgs::new();
        for (name, value) in values {
            args.set(name.clone(), value.clone());
        }
        for b in [&self.primary, &self.fallback].into_iter().flatten() {
            if let Some(pattern) = b.get_message(key).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                let text = b.format_pattern(pattern, Some(&args), &mut errors).into_owned();
                problems.extend(errors.iter().map(|e| format!("{key}: {e}")));
                return text;
            }
        }
        problems.push(format!("{}: no such message", printable(key)));
        printable(key)
    }
}

pub fn keys(source: &str) -> Vec<String> {
    source
        .lines()
        .filter(|l| !l.starts_with([' ', '\t', '#', '-', '.', '[', '*', '}']))
        .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim().to_string()))
        .filter(|k| !k.is_empty() && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
        .collect()
}

pub fn variables(source: &str, key: &str) -> Vec<String> {
    let mut inside = false;
    let mut found = Vec::new();
    for line in source.lines() {
        if line.starts_with('#') {
            inside = false;
            continue;
        }
        let starts_entry = !line.is_empty() && !line.starts_with([' ', '\t', '}']);
        if starts_entry {
            inside = line.split_once('=').is_some_and(|(k, _)| k.trim() == key);
        }
        if !inside {
            continue;
        }
        let mut depth = 0usize;
        for (at, c) in line.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                '$' if depth > 0 => {
                    let name: String = line[at + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
                    if name.starts_with(|first: char| first.is_ascii_alphabetic()) && !found.contains(&name) {
                        found.push(name);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const RU: &str =
        "touches = { $count ->\n    [one] { $count } касание\n    [few] { $count } касания\n   *[many] { $count } касаний\n}\nhello = привет, { $name }\n";
    const EN: &str = "touches = { $count ->\n    [one] { $count } touch\n   *[other] { $count } touches\n}\nhello = hello, { $name }\nonly-en = english\n";
    const SOURCES: [(&str, &str); 2] = [("ru", RU), ("en", EN)];

    fn ru() -> Catalog {
        Catalog::for_languages(&SOURCES, &["ru".into()])
    }

    #[test]
    fn plurals_follow_language_rules() {
        let c = ru();
        let t = |n: usize| c.render(&Msg::new("touches").num("count", n));
        assert_eq!((t(1), t(3), t(5), t(21)), ("1 касание".into(), "3 касания".into(), "5 касаний".into(), "21 касание".into()));
        let en = Catalog::for_languages(&SOURCES, &["de".into()]);
        assert_eq!(en.language(), "en");
        assert_eq!(en.render(&Msg::new("touches").num("count", 1)), "1 touch");
    }

    #[test]
    fn text_arguments_are_never_reinterpreted_as_numbers() {
        let c = ru();
        for name in ["007", "nan", "inf", "1e5", "0x10", " 5"] {
            assert_eq!(c.render(&Msg::new("hello").arg("name", name)), format!("привет, {name}"));
        }
    }

    #[test]
    fn missing_key_falls_back_then_shows_key() {
        let c = ru();
        assert!(c.is_loaded() && c.has("hello") && !c.has("only-en"));
        assert_eq!(c.text("only-en"), "english");
        assert_eq!(c.render(&Msg::new("nope").num("x", 1)), "nope x=1");
    }

    #[test]
    fn nested_keys_are_rendered_with_parent_args() {
        let sources = [("ru", "outer = итог: { $inner }\ninner = вложено { $x }\n"), ("en", "outer = total: { $inner }\n")];
        let c = Catalog::for_languages(&sources, &["ru".into()]);
        assert_eq!(c.render(&Msg::new("outer").sub("inner", "inner").num("x", 7)), "итог: вложено 7");
        assert_eq!(c.render(&Msg::new("outer").arg("inner", "inner")), "итог: inner");
        assert_eq!(c.render(&Msg::new("outer").sub("inner", "absent")), "итог: absent");
        assert_eq!(c.render(&Msg::new("outer").nest("inner", Msg::new("inner").num("x", 9))), "итог: вложено 9");
        assert_eq!(
            c.render(&Msg::new("outer").num("x", 1).nest("inner", Msg::new("inner").num("x", 9)).nest("unused", Msg::new("inner").num("x", 5))),
            "итог: вложено 9"
        );
        assert!(c.try_render(&Msg::new("outer").num("x", 3).nest("inner", Msg::new("inner"))).is_err());
        assert_eq!(c.try_render(&Msg::new("outer").num("x", 3).sub("inner", "inner")).as_deref(), Ok("итог: вложено 3"));
        let mut deep = Msg::new("inner").num("x", 1);
        for _ in 0..8 {
            deep = Msg::new("outer").nest("inner", deep);
        }
        assert!(c.try_render(&deep).is_err());
    }

    #[test]
    fn several_sources_per_language_are_merged() {
        let c = Catalog::for_languages(&[("ru", "a = один\n"), ("ru", "b = два\n"), ("en", "a = one\n")], &["ru".into()]);
        assert_eq!((c.text("a"), c.text("b")), ("один".into(), "два".into()));
    }

    #[test]
    fn broken_or_duplicated_sources_do_not_load() {
        assert!(!Catalog::for_languages(&[("ru", "a = { $x\n"), ("en", "a = ok\n")], &["ru".into()]).is_loaded());
        assert!(!Catalog::for_languages(&[("ru", "a = 1\n"), ("ru", "a = 2\n")], &["ru".into()]).is_loaded());
    }

    #[test]
    fn locale_variables_follow_gettext_rules() {
        let env =
            |pairs: &'static [(&'static str, &'static str)]| move |name: &str| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string());
        assert_eq!(languages_from(env(&[("LANG", "ru_RU.UTF-8")])), ["ru"]);
        assert_eq!(languages_from(env(&[("LANG", "ru_RU.UTF-8"), ("LC_ALL", "C")])), Vec::<String>::new());
        assert_eq!(languages_from(env(&[("LANG", "ru_RU.UTF-8"), ("LC_MESSAGES", "POSIX")])), Vec::<String>::new());
        assert_eq!(languages_from(env(&[("LANGUAGE", "ru"), ("LC_ALL", "C")])), Vec::<String>::new());
        assert_eq!(languages_from(env(&[("LANGUAGE", "de:ru"), ("LANG", "en_US.UTF-8")])), ["de", "ru"]);
        assert_eq!(languages_from(env(&[("LANGUAGE", "xx"), ("LANG", "ru_RU.UTF-8")])), ["xx"]);
        assert_eq!(languages_from(env(&[("LANGUAGE", "C"), ("LANG", "ru_RU.UTF-8")])), Vec::<String>::new());
        assert_eq!(languages_from(env(&[("LANGUAGE", "de:C:ru"), ("LANG", "ru_RU.UTF-8")])), ["de"]);
        assert_eq!(languages_from(env(&[("LANGUAGE", "de:POSIX.UTF-8:ru"), ("LANG", "ru_RU.UTF-8")])), ["de"]);
        assert_eq!(languages_from(env(&[("LANGUAGE", "../x:ru:ru_RU"), ("LANG", "en_US.UTF-8")])), ["ru"]);
        assert_eq!(languages_from(env(&[("LANGUAGE", ""), ("LANG", "en_US.UTF-8")])), ["en"]);
        assert_eq!(languages_from(env(&[("LC_MESSAGES", "en_GB"), ("LANG", "ru_RU")])), ["en"]);
        assert_eq!(languages_from(env(&[("LC_ALL", ""), ("LANG", "ru_RU")])), ["ru"]);
        assert_eq!(languages_from(env(&[("LANGUAGE", "ru")])), Vec::<String>::new());
        assert_eq!(languages_from(env(&[])), Vec::<String>::new());
    }

    #[test]
    fn untrusted_values_cannot_drive_the_terminal() {
        let c = Catalog::for_languages(&SOURCES, &["ru".into()]);
        let shown = c.render(&Msg::new("hello").arg("name", "a\x1b[2J\rb\nc\u{202e}d\u{2066}e"));
        assert!(!shown.chars().any(|ch| ch.is_control() || ch == '\u{202e}' || ch == '\u{2066}'), "{shown:?}");
        assert!(shown.contains("a\u{fffd}[2J\u{fffd}b\u{fffd}c\u{fffd}d\u{fffd}e"));
        assert_eq!(printable("обычный текст — ok ✓"), "обычный текст — ok ✓");
        let invisible = "\u{200b}\u{2060}\u{feff}\u{00ad}\u{2028}\u{2029}\u{e0041}\u{206a}\u{fff9}";
        assert!(printable(invisible).chars().all(|ch| ch == char::REPLACEMENT_CHARACTER));
        let more = "\u{034f}\u{115f}\u{1160}\u{17b4}\u{180b}\u{180f}\u{3164}\u{fe0f}\u{ffa0}\u{fff0}\u{16fe4}\u{1bca0}\u{1d159}\u{1d173}\u{e0100}\u{e0fff}\u{2800}\u{e000}\u{f8ff}\u{f0000}\u{10fffd}\u{fdd0}\u{fffe}\u{1ffff}";
        assert!(printable(more).chars().all(|ch| ch == char::REPLACEMENT_CHARACTER), "{:?}", printable(more));
        assert_eq!(printable("a\u{00a0}b\u{2003}c\u{202f}d\u{3000}e\u{1680}f"), "a b c d e f");
        let stacked = format!("e{}!", "\u{0301}".repeat(2000));
        assert_eq!(printable(&stacked), format!("e{}!", "\u{0301}".repeat(MAX_STACK)));
        assert_eq!(printable("ё й и\u{0306} e\u{0301}\u{0302} שָׁלוֹם"), "ё й и\u{0306} e\u{0301}\u{0302} שָׁלוֹם");
        for flood in ["\u{0e49}", "\u{0591}", "\u{0951}", "\u{a66f}", "\u{3099}"] {
            assert_eq!(printable(&format!("ก{}", flood.repeat(200))).chars().count(), 1 + MAX_STACK, "{flood:?}");
        }
        assert_eq!(printable(&format!("ก{}", "\u{0301}\u{0301}\u{0301}\u{0301}\u{0e49}".repeat(50))).chars().count(), 1 + MAX_STACK);
        let tallest = |text: &str| {
            let (mut run, mut most) = (0, 0);
            for c in printable(text).chars() {
                run = if stacking(c) {
                    run + 1
                } else if hidden(c) {
                    run
                } else {
                    0
                };
                most = most.max(run);
            }
            most
        };
        assert_eq!(tallest(&format!("❤{}", "\u{0301}\u{0301}\u{0301}\u{0301}\u{fe0f}".repeat(50))), MAX_STACK);
        assert_eq!(tallest(&format!("ก{}ก", "\u{0e49}\u{0e49}\u{0e49}\u{0e49}\u{0e49}\u{200d}".repeat(50))), 5);
        assert_eq!(tallest(&format!("و{}و", "\u{064b}\u{064b}\u{064b}\u{064b}\u{064b}\u{200d}".repeat(50))), MAX_STACK);
        assert_eq!(tallest(&format!("e{}", "\u{0301}\u{0301}\u{0301}\u{0301}\u{0301}\u{0301}\u{0301}\u{0301}\u{1161}".repeat(200))), MAX_STACK);
        assert_eq!(printable("한글 \u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}"), "한글 \u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}");
    }

    fn longest_link_run(text: &str) -> usize {
        let (mut run, mut longest, mut linking) = (0, 0, false);
        for c in text.chars() {
            if stacking(c) {
                linking |= stacker(c);
                continue;
            }
            run = if linking { run + 1 } else { 0 };
            longest = longest.max(run);
            linking = false;
        }
        longest
    }

    #[test]
    fn a_chain_of_joining_signs_and_letters_cannot_grow_a_column_either() {
        for (letter, sign) in [
            ('ꦏ', '\u{a9c0}'),
            ('\u{11a0b}', '\u{11a47}'),
            ('\u{11103}', '\u{11133}'),
            ('\u{11a5c}', '\u{11a99}'),
            ('\u{1158e}', '\u{115bf}'),
            ('ក', '\u{17d2}'),
        ] {
            let pair = format!("{sign}{letter}");
            let chain = printable(&format!("{letter}{}", pair.repeat(400)));
            assert!(chain.contains(&pair.repeat(MAX_JOINED)), "{sign:?}");
            assert!(!chain.contains(&pair.repeat(MAX_JOINED + 1)), "{sign:?}");
            assert_eq!(chain.chars().filter(|c| *c == letter).count(), 401, "{sign:?}");
            assert_eq!(chain.chars().filter(|c| *c == sign).count(), 400 - 400 / (MAX_JOINED + 1), "{sign:?}");
        }
        for (letter, sign, dot) in [('\u{1158e}', '\u{115bf}', '\u{115c0}'), ('ꦏ', '\u{a9c0}', '\u{a9b3}')] {
            let link = format!("{sign}{dot}{letter}");
            let chain = printable(&format!("{letter}{}", link.repeat(400)));
            assert!(chain.contains(&link.repeat(MAX_JOINED)) && !chain.contains(&link.repeat(MAX_JOINED + 1)), "{sign:?}");
        }
        for (letter, sign, dot) in [('ꦏ', '\u{a9c0}', '\u{a9b3}'), ('\u{1158e}', '\u{115bf}', '\u{115c0}'), ('\u{11a0b}', '\u{11a47}', '\u{11a33}')] {
            for dots in 0..=2 * MAX_STACK {
                let link = format!("{sign}{}{letter}", dot.to_string().repeat(dots));
                let chain = printable(&format!("{letter}{}", link.repeat(60)));
                assert!(longest_link_run(&chain) <= MAX_JOINED, "{sign:?} with {dots} marks: {}", longest_link_run(&chain));
                let marks_first = printable(&format!("{letter}{}", format!("{}{sign}{letter}", dot.to_string().repeat(dots)).repeat(60)));
                assert!(longest_link_run(&marks_first) <= MAX_JOINED, "{sign:?} after {dots} marks");
            }
        }
        let towering = printable(&format!("ᯀ{}", "\u{1bee}".repeat(200)));
        assert_eq!(towering.chars().count(), 1 + MAX_STACK);
        let words = "क्षत्रिय कार्त्स्न्य স্ত্রী ស្ត្រី தமிழ் ஃப்ர்ஸ்ட்ன்ஸ் മലയാളം ဗမာစာ တော်တော်များများ ပေါ်ပေါ်ကျော် သတ္တဝါ कि की कौ";
        assert_eq!(printable(words), words);
        let halants = format!("क{}", "\u{094d}क".repeat(40));
        assert_eq!(printable(&halants), halants);
        assert_eq!(printable("क\u{094d}a\u{0301}"), "क\u{094d}a\u{0301}");
        assert_eq!(printable("ꦏ\u{a9c0}a\u{0301}"), "ꦏ\u{a9c0}a\u{0301}");
    }

    #[test]
    fn joiners_and_selectors_stay_where_writing_needs_them_and_nowhere_else() {
        for kept in [
            "❤\u{fe0f}",
            "1\u{fe0f}\u{20e3}",
            "👨\u{200d}👩\u{200d}👧",
            "❤\u{fe0f}\u{200d}🔥",
            "می\u{200c}خواهم",
            "क\u{094d}\u{200d}ष",
            "葛\u{e0100}",
            "ᠠ\u{180b}",
            "\u{0600}١٢٣ \u{06dd} \u{110bd}",
            "©\u{fe0f} ℹ\u{fe0f} ‼\u{fe0f} Ⓜ\u{fe0f} 🅰\u{fe0f} 🅿\u{fe0f}",
            "র\u{200d}\u{09cd}য",
            "Tiếng Việt, ภาษาไทย, བོད་སྐད, हिन्दी, العَرَبِيَّة, 日本語",
        ] {
            assert_eq!(printable(kept), kept, "{kept:?}");
        }
        let replaced = |text: &str| printable(text).replace(char::REPLACEMENT_CHARACTER, "?");
        assert_eq!(replaced("pa\u{fe0f}ss"), "pa?ss");
        assert_eq!(replaced("a\u{200d}b"), "a?b");
        assert_eq!(replaced("\u{200c}می"), "?می");
        assert_eq!(replaced("می\u{200c} خواهم"), "می? خواهم");
        assert_eq!(replaced("a \u{fe0f}b"), "a ?b");
        assert_eq!(replaced("1\u{fe0f}2"), "1?2");
        assert_eq!(replaced("👨\u{200d}\u{200d}👩"), "👨??👩");
        assert_eq!(replaced("\u{202e}\u{fe0f}x"), "??x");
        assert_eq!(replaced("x\u{e0100}"), "x?");
        assert_eq!(replaced("ᠠ\u{180b}a\u{180b}"), "ᠠ\u{180b}a?");
        assert_eq!(replaced("И\u{200d}ван"), "И?ван");
        assert_eq!(replaced("И\u{fe0f}ван"), "И?ван");
        assert_eq!(replaced("é\u{e0100}"), "é?");
        assert_eq!(replaced("❤\u{fe0f}\u{fe0f}\u{fe0f}"), "❤\u{fe0f}??");
        assert_eq!(replaced("葛\u{e0100}\u{e0101}"), "葛\u{e0100}?");
        assert_eq!(replaced("ᠠ\u{180b}\u{180c}"), "ᠠ\u{180b}?");
        assert_eq!(replaced(&format!("❤{}", "\u{fe0f}".repeat(100_000))).chars().filter(|c| *c == '?').count(), 99_999);
        for (base, mark) in [('ب', '\u{064e}'), ('ក', '\u{17d2}'), ('❤', '\u{fe20}')] {
            let pair = format!("\u{200d}{mark}");
            let run = printable(&format!("{base}{}", pair.repeat(300)));
            assert!(!run.contains("\u{200d}\u{200d}"), "{base:?}");
        }
    }

    #[test]
    fn the_debugging_form_of_a_message_cannot_be_forged_by_a_value() {
        let forged = Msg::new("usb-error").arg("detail", "stall status=ok user=root (x)").num("n", 3).arg("empty", "").arg("path", "/dev/bus/usb/003/004");
        assert_eq!(forged.to_string(), "usb-error detail=\"stall status=ok user=root (x)\" n=3 empty=\"\" path=/dev/bus/usb/003/004");
        let quoting = Msg::new("k").arg("v", "say \"hi\" \\ bye");
        assert_eq!(quoting.to_string(), "k v=\"say \\\"hi\\\" \\\\ bye\"");
        let nested = Msg::new("outer").nest("inner", Msg::new("inner-key").arg("a", "b c"));
        assert_eq!(nested.to_string(), "outer inner=(inner-key a=\"b c\")");
    }

    #[test]
    fn no_way_of_printing_a_message_lets_raw_input_through() {
        let c = Catalog::for_languages(&SOURCES, &["ru".into()]);
        let nasty = "a\x1b[2J\u{202e}b";
        let clean = |text: &str| !text.chars().any(|ch| ch.is_control() || ch == '\u{202e}');
        let as_number = Msg::new("hello").num("name", nasty);
        assert!(clean(&c.render(&as_number)) && c.try_render(&as_number).is_err());
        let as_key = Msg::new("hello").sub("name", nasty);
        assert!(clean(&c.render(&as_key)) && c.try_render(&as_key).is_err());
        let unknown = Msg::new(nasty).arg(nasty, nasty);
        assert!(clean(&c.render(&unknown)) && clean(&unknown.to_string()) && clean(&Fail(unknown).to_string()));
        let mut deep = Msg::new(nasty);
        for _ in 0..8 {
            deep = Msg::new("hello").nest("name", deep);
        }
        assert!(clean(&c.render(&deep)));
    }

    #[test]
    fn numbers_keep_their_digits() {
        let c = Catalog::for_languages(&SOURCES, &["ru".into()]);
        let shown = |value: &str| c.render(&Msg::new("hello").num("name", value));
        assert_eq!(shown("18446744073709551615"), "привет, 18446744073709551615");
        assert_eq!(shown("9007199254740993"), "привет, 9007199254740993");
        assert_eq!(shown("-42"), "привет, -42");
        assert_eq!(shown("0.5"), "привет, 0.5");
        assert!(c.try_render(&Msg::new("hello").num("name", "18446744073709551615")).is_ok());
        for odd in ["1e3", "inf", "NaN", "", "-", "1.", ".5", "0x10", "1 000"] {
            assert_eq!(shown(odd), format!("привет, {odd}"), "{odd:?}");
            assert!(c.try_render(&Msg::new("hello").num("name", odd)).is_err(), "{odd:?}");
        }
        assert_eq!(c.render(&Msg::new("touches").num("count", "123456789012345")), "123456789012345 касаний");
        for exact in ["007", "-000.500", "00", "0.30000000000000004", "99999999999.9999", "-0.5", "0", "10"] {
            assert_eq!(shown(exact), format!("привет, {exact}"), "{exact:?}");
            assert!(c.try_render(&Msg::new("hello").num("name", exact)).is_ok(), "{exact:?}");
        }
        assert_eq!(c.render(&Msg::new("touches").num("count", "007")), "007 касаний");
        assert_eq!(c.render(&Msg::new("touches").num("count", "21")), "21 касание");
    }

    #[test]
    fn missing_variables_and_messages_are_reported() {
        const NESTED: [(&str, &str); 1] = [("en", "outer = total: { $inner }\ninner = nested { $x }\nhello = hello, { $name }\n")];
        let c = Catalog::for_languages(&NESTED, &["en".into()]);
        assert_eq!(c.try_render(&Msg::new("hello").arg("name", "мир")).as_deref(), Ok("hello, мир"));
        assert!(c.try_render(&Msg::new("hello")).is_err());
        assert!(c.try_render(&Msg::new("no-such-key")).is_err());
        assert!(c.try_render(&Msg::new("outer").sub("inner", "absent")).is_err());
        assert!(c.try_render(&Msg::new("outer").nest("inner", Msg::new("inner"))).is_err());
    }

    #[test]
    fn language_tags_are_normalised_and_filtered() {
        assert_eq!(language_of("ru_RU.UTF-8").as_deref(), Some("ru"));
        assert_eq!(language_of("en-GB").as_deref(), Some("en"));
        assert_eq!(language_of("C"), None);
        assert_eq!(language_of("POSIX"), None);
        assert_eq!(language_of("../../etc"), None);
        let c = Catalog::for_languages(&SOURCES, &["fr".into(), "ru".into()]);
        assert_eq!(c.language(), "ru");
    }

    #[test]
    fn catalog_is_shareable_between_threads() {
        fn assert_sync<T: Send + Sync>(_: &T) {}
        let c = ru();
        assert_sync(&c);
        std::thread::scope(|s| {
            s.spawn(|| assert_eq!(c.render(&Msg::new("touches").num("count", 2)), "2 касания"));
        });
    }

    #[test]
    fn source_introspection_lists_keys_and_variables() {
        assert_eq!(keys(RU), vec!["touches".to_string(), "hello".to_string()]);
        assert_eq!(variables(RU, "touches"), vec!["count".to_string()]);
        assert_eq!(variables(EN, "hello"), vec!["name".to_string()]);
        let commented = "a = { $x } costs $5\n# b uses $count\nb = { $count }\n";
        assert_eq!(variables(commented, "a"), vec!["x".to_string()]);
        assert_eq!(variables(commented, "b"), vec!["count".to_string()]);
    }

    #[test]
    fn what_a_catalog_says_is_sanitised_too_and_caps_hold_across_placeables() {
        let source = "bell = ring{\"\\u001b\"}]0;x{\"\\u0007\"} the bell\nlines = one\n    two\nmarks = { $a }{ $b }{ $c }\n";
        let catalog = Catalog::for_languages(&[("en", source)], &["en".into()]);
        let bell = catalog.render(&Msg::new("bell"));
        assert!(!bell.contains('\u{1b}') && !bell.contains('\u{7}'), "{bell:?}");
        assert_eq!(catalog.render(&Msg::new("lines")), "one\ntwo");
        let accents = "\u{301}".repeat(8);
        let stacked = catalog.render(&Msg::new("marks").arg("a", &accents).arg("b", &accents).arg("c", &accents));
        assert!(stacked.chars().filter(|c| *c == '\u{301}').count() <= MAX_STACK, "{}", stacked.chars().count());
        let hiding = format!("e{}", format!("{accents}\u{70f}").repeat(50));
        assert!(printable(&hiding).chars().filter(|c| *c == '\u{301}').count() <= MAX_STACK);
    }

    #[test]
    fn errors_carry_their_message() {
        let e: Box<dyn std::error::Error> = Fail::new("hello").arg("name", "x").into();
        assert_eq!(as_msg(e.as_ref(), "other"), Msg::new("hello").arg("name", "x"));
        let io: Box<dyn std::error::Error> = std::io::Error::other("boom").into();
        assert_eq!(as_msg(io.as_ref(), "other"), Msg::new("other").arg("detail", "boom"));
    }
}
