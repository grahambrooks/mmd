//! gitGraph parsing.
//!
//! Grammar subset per `DESIGN.md`, checked against mermaid's
//! `packages/parser/src/language/gitGraph/gitGraph.langium`; the semantic
//! checks (what may be merged or cherry-picked) follow mermaid's
//! `packages/mermaid/src/diagrams/git/gitGraphAst.ts`.

use crate::{Error, strip_comment};

/// Which way time runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Left to right, one horizontal lane per branch.
    #[default]
    LR,
    /// Top to bottom, one vertical lane per branch.
    TB,
    /// Bottom to top, one vertical lane per branch.
    BT,
}

/// The mark a commit is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommitType {
    #[default]
    Normal,
    Reverse,
    Highlight,
    /// A ring: a merge commit without its own `type:`.
    Merge,
    /// A cherry-pick commit.
    CherryPick,
}

/// The statement that made a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    #[default]
    Commit,
    /// `merge`: the second parent is the merged branch's head.
    Merge,
    /// `cherry-pick`: the second parent is the picked commit.
    CherryPick,
}

#[derive(Debug, Clone, Default)]
pub struct Commit {
    /// Unique within the graph; `cherry-pick id:` refers to it.
    pub id: String,
    /// The text drawn beside the commit, if any. Cherry-picks and merges
    /// without an `id:` have none, as in mermaid.
    pub label: Option<String>,
    /// `msg:`. Mermaid parses it but never draws it, so neither does this.
    pub message: Option<String>,
    pub tags: Vec<String>,
    pub kind: CommitType,
    pub origin: Origin,
    /// Index into [`GitGraph::branches`].
    pub branch: usize,
    /// Earlier commits this one follows. A merge or cherry-pick has two; the
    /// first is always the previous head of the commit's own branch.
    pub parents: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    /// `order:`, which places the branch's lane.
    pub order: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct GitGraph {
    pub direction: Direction,
    /// `title`, drawn above the graph.
    pub title: Option<String>,
    /// `accTitle:`; stored, not drawn.
    pub acc_title: Option<String>,
    /// `accDescr`; stored, not drawn.
    pub acc_descr: Option<String>,
    pub branches: Vec<Branch>,
    pub commits: Vec<Commit>,
}

impl GitGraph {
    /// Branch indices in lane order, first lane first.
    ///
    /// As in mermaid: `main` has order 0; a branch without `order:` sorts
    /// after order 0 and before order 1, in declaration order; explicit
    /// orders sort numerically, ties in declaration order.
    pub fn lane_order(&self) -> Vec<usize> {
        let key = |index: usize| match self.branches[index].order {
            Some(order) => (u64::from(order) * 2, index),
            None if index == 0 => (0, 0),
            None => (1, index),
        };
        let mut lanes: Vec<usize> = (0..self.branches.len()).collect();
        lanes.sort_by_key(|&index| key(index));
        lanes
    }
}

/// Parse a gitGraph source.
pub fn parse(source: &str) -> Result<GitGraph, Error> {
    let mut parser = Parser::default();
    parser.graph.branches.push(Branch {
        name: "main".to_string(),
        order: None,
    });
    parser.heads.push(None);

    let mut seen_header = false;
    // An open `accDescr {` block: where it started, and its text so far.
    let mut description: Option<(usize, Vec<String>)> = None;

    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;

        if let Some((_, lines)) = &mut description {
            match raw.find('}') {
                Some(end) => {
                    lines.push(raw[..end].trim().to_string());
                    let text = lines.join("\n").trim().to_string();
                    parser.graph.acc_descr = Some(text);
                    description = None;
                    let tail = strip_comment(&raw[end + 1..]).trim();
                    if !tail.is_empty() {
                        return Err(Error::new(
                            line_number,
                            format!("unexpected `{tail}` after `accDescr {{ … }}`"),
                        ));
                    }
                }
                None => lines.push(raw.trim().to_string()),
            }
            continue;
        }

        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if !seen_header {
            parser.graph.direction = header(line, line_number)?;
            seen_header = true;
            continue;
        }

        if let Some(rest) = keyword_rest(line, "accDescr") {
            if let Some(body) = rest.strip_prefix('{') {
                match body.find('}') {
                    Some(end) => {
                        parser.graph.acc_descr = Some(body[..end].trim().to_string());
                        let tail = body[end + 1..].trim();
                        if !tail.is_empty() {
                            return Err(Error::new(
                                line_number,
                                format!("unexpected `{tail}` after `accDescr {{ … }}`"),
                            ));
                        }
                    }
                    None => description = Some((line_number, vec![body.trim().to_string()])),
                }
                continue;
            }
            let text = rest.strip_prefix(':').ok_or_else(|| {
                Error::new(
                    line_number,
                    "expected `accDescr: <text>` or `accDescr { … }`",
                )
            })?;
            parser.graph.acc_descr = Some(text.trim().to_string());
            continue;
        }
        if let Some(rest) = keyword_rest(line, "accTitle") {
            let text = rest
                .strip_prefix(':')
                .ok_or_else(|| Error::new(line_number, "expected `accTitle: <text>`"))?;
            parser.graph.acc_title = Some(text.trim().to_string());
            continue;
        }

        let (keyword, rest) = split_keyword(line);
        match keyword {
            "title" => parser.graph.title = Some(rest.trim().to_string()).filter(|t| !t.is_empty()),
            "commit" => parser.commit(rest, line_number)?,
            "branch" => parser.branch(rest, line_number)?,
            "checkout" | "switch" => parser.checkout(keyword, rest, line_number)?,
            "merge" => parser.merge(rest, line_number)?,
            "cherry-pick" => parser.cherry_pick(rest, line_number)?,
            other => {
                return Err(Error::new(
                    line_number,
                    format!(
                        "unexpected `{other}`; expected commit, branch, checkout, merge or cherry-pick"
                    ),
                ));
            }
        }
    }

    if let Some((line, _)) = description {
        return Err(Error::new(line, "`accDescr {` is never closed with `}`"));
    }
    if !seen_header {
        return Err(Error::new(1, "expected `gitGraph`"));
    }
    Ok(parser.graph)
}

/// `gitGraph`, `gitGraph:`, or `gitGraph LR|TB|BT`, with an optional colon.
fn header(line: &str, line_number: usize) -> Result<Direction, Error> {
    let rest = line
        .strip_prefix("gitGraph")
        .ok_or_else(|| Error::new(line_number, "expected `gitGraph`"))?;
    if !rest.is_empty() && !rest.starts_with([' ', '\t', ':']) {
        return Err(Error::new(line_number, "expected `gitGraph`"));
    }
    let rest = rest.trim();
    let rest = rest.strip_suffix(':').unwrap_or(rest).trim();
    match rest {
        "" | "LR" => Ok(Direction::LR),
        "TB" => Ok(Direction::TB),
        "BT" => Ok(Direction::BT),
        other => Err(Error::new(
            line_number,
            format!("unexpected `{other}` after `gitGraph`; expected a direction, LR, TB or BT"),
        )),
    }
}

/// The text after `keyword` when the line starts with it as a whole word.
fn keyword_rest<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    if rest.is_empty() || rest.starts_with([' ', '\t', ':', '{']) {
        Some(rest.trim_start())
    } else {
        None
    }
}

#[derive(Default)]
struct Parser {
    graph: GitGraph,
    /// The newest commit on each branch.
    heads: Vec<Option<usize>>,
    current: usize,
    auto_id: usize,
}

impl Parser {
    fn push(&mut self, commit: Commit) {
        self.graph.commits.push(commit);
        self.heads[self.current] = Some(self.graph.commits.len() - 1);
    }

    fn find_commit(&self, id: &str) -> Option<usize> {
        self.graph.commits.iter().position(|commit| commit.id == id)
    }

    fn find_branch(&self, name: &str) -> Option<usize> {
        self.graph
            .branches
            .iter()
            .position(|branch| branch.name == name)
    }

    fn unique_id(&self, id: &str, line: usize) -> Result<(), Error> {
        if self.find_commit(id).is_some() {
            return Err(Error::new(
                line,
                format!("commit id `{id}` already exists; ids must be unique"),
            ));
        }
        Ok(())
    }

    fn branch_name(&self, branch: usize) -> &str {
        &self.graph.branches[branch].name
    }

    fn commit(&mut self, rest: &str, line: usize) -> Result<(), Error> {
        let mut commit = Commit {
            branch: self.current,
            parents: self.heads[self.current].into_iter().collect(),
            ..Commit::default()
        };
        let mut id = None;
        for (key, value) in attributes(rest, line, "commit", &["id", "msg", "tag", "type"])? {
            match key {
                "id" => id = Some(value),
                "msg" => commit.message = Some(value),
                "tag" if value.is_empty() => {}
                "tag" => commit.tags.push(value),
                _ => commit.kind = commit_type(&value, line)?,
            }
        }
        commit.id = match id {
            Some(id) => {
                self.unique_id(&id, line)?;
                id
            }
            None => loop {
                self.auto_id += 1;
                let candidate = format!("c{}", self.auto_id);
                if self.find_commit(&candidate).is_none() {
                    break candidate;
                }
            },
        };
        commit.label = Some(commit.id.clone());
        self.push(commit);
        Ok(())
    }

    fn branch(&mut self, rest: &str, line: usize) -> Result<(), Error> {
        let (name, rest) = take_value(rest, line)?;
        if name.is_empty() {
            return Err(Error::new(line, "`branch` needs a branch name"));
        }
        let mut order = None;
        for (_, value) in attributes(rest, line, "branch", &["order"])? {
            order = Some(value.parse::<u32>().map_err(|_| {
                Error::new(
                    line,
                    format!("`order:` takes a whole number, got `{value}`"),
                )
            })?);
        }
        if self.find_branch(&name).is_some() {
            return Err(Error::new(
                line,
                format!("branch `{name}` already exists; use `checkout {name}` to switch to it"),
            ));
        }
        self.graph.branches.push(Branch { name, order });
        // A new branch starts at the current head and is checked out.
        self.heads.push(self.heads[self.current]);
        self.current = self.graph.branches.len() - 1;
        Ok(())
    }

    fn checkout(&mut self, keyword: &str, rest: &str, line: usize) -> Result<(), Error> {
        let (name, tail) = take_value(rest, line)?;
        if name.is_empty() {
            return Err(Error::new(line, format!("`{keyword}` needs a branch name")));
        }
        if !tail.trim().is_empty() {
            return Err(Error::new(
                line,
                format!("`{keyword}` takes one branch name, got `{}`", rest.trim()),
            ));
        }
        self.current = self
            .find_branch(&name)
            .ok_or_else(|| Error::new(line, format!("unknown branch `{name}`")))?;
        Ok(())
    }

    fn merge(&mut self, rest: &str, line: usize) -> Result<(), Error> {
        let (name, rest) = take_value(rest, line)?;
        if name.is_empty() {
            return Err(Error::new(line, "`merge` needs a branch name"));
        }
        let mut id = None;
        let mut commit = Commit {
            kind: CommitType::Merge,
            origin: Origin::Merge,
            branch: self.current,
            ..Commit::default()
        };
        for (key, value) in attributes(rest, line, "merge", &["id", "tag", "type"])? {
            match key {
                "id" => id = Some(value),
                "tag" if value.is_empty() => {}
                "tag" => commit.tags.push(value),
                _ => commit.kind = commit_type(&value, line)?,
            }
        }

        let source = self
            .find_branch(&name)
            .ok_or_else(|| Error::new(line, format!("unknown branch `{name}`")))?;
        if source == self.current {
            return Err(Error::new(
                line,
                format!("cannot merge branch `{name}` into itself"),
            ));
        }
        let head = self.heads[self.current].ok_or_else(|| {
            Error::new(
                line,
                format!(
                    "cannot merge into `{}`: it has no commits",
                    self.branch_name(self.current)
                ),
            )
        })?;
        let other = self.heads[source]
            .ok_or_else(|| Error::new(line, format!("branch `{name}` has no commits to merge")))?;
        if head == other {
            return Err(Error::new(
                line,
                format!(
                    "branch `{name}` has no commits of its own to merge into `{}`",
                    self.branch_name(self.current)
                ),
            ));
        }
        commit.parents = vec![head, other];
        match id {
            Some(id) => {
                self.unique_id(&id, line)?;
                commit.label = Some(id.clone());
                commit.id = id;
            }
            None => {
                // Mermaid labels a merge only when it is given an id.
                commit.id = format!("merge-{}", self.graph.commits.len());
                commit.label = None;
            }
        }
        self.push(commit);
        Ok(())
    }

    fn cherry_pick(&mut self, rest: &str, line: usize) -> Result<(), Error> {
        let mut id = None;
        let mut parent = None;
        let mut tags: Option<Vec<String>> = None;
        for (key, value) in attributes(rest, line, "cherry-pick", &["id", "tag", "parent"])? {
            match key {
                "id" => id = Some(value),
                "parent" => parent = Some(value),
                _ => tags.get_or_insert_with(Vec::new).push(value),
            }
        }

        let id = id.ok_or_else(|| {
            Error::new(line, "`cherry-pick` needs `id:` naming the commit to pick")
        })?;
        let source = self
            .find_commit(&id)
            .ok_or_else(|| Error::new(line, format!("unknown commit `{id}`")))?;
        let picked = &self.graph.commits[source];
        if let Some(parent) = &parent {
            let is_parent = picked
                .parents
                .iter()
                .any(|&index| self.graph.commits[index].id == *parent);
            if !is_parent {
                return Err(Error::new(
                    line,
                    format!("`parent: \"{parent}\"` is not a parent of commit `{id}`"),
                ));
            }
        } else if picked.origin == Origin::Merge {
            return Err(Error::new(
                line,
                format!(
                    "commit `{id}` is a merge; `cherry-pick` needs `parent:` naming one of its parents"
                ),
            ));
        }
        if picked.branch == self.current {
            return Err(Error::new(
                line,
                format!(
                    "commit `{id}` is already on branch `{}`",
                    self.branch_name(self.current)
                ),
            ));
        }
        let head = self.heads[self.current].ok_or_else(|| {
            Error::new(
                line,
                format!(
                    "cannot cherry-pick onto `{}`: it has no commits",
                    self.branch_name(self.current)
                ),
            )
        })?;

        // Mermaid tags a cherry-pick with its source unless told otherwise;
        // an empty `tag: ""` means no tag at all.
        let tags = match tags {
            Some(tags) => tags.into_iter().filter(|tag| !tag.is_empty()).collect(),
            None => vec![match (&parent, picked.origin) {
                (Some(parent), Origin::Merge) => format!("cherry-pick:{id}|parent:{parent}"),
                _ => format!("cherry-pick:{id}"),
            }],
        };
        let commit = Commit {
            id: format!("cherry-pick-{}", self.graph.commits.len()),
            label: None,
            message: None,
            tags,
            kind: CommitType::CherryPick,
            origin: Origin::CherryPick,
            branch: self.current,
            parents: vec![head, source],
        };
        self.push(commit);
        Ok(())
    }
}

fn commit_type(value: &str, line: usize) -> Result<CommitType, Error> {
    match value {
        "NORMAL" => Ok(CommitType::Normal),
        "REVERSE" => Ok(CommitType::Reverse),
        "HIGHLIGHT" => Ok(CommitType::Highlight),
        other => Err(Error::new(
            line,
            format!("unknown commit type `{other}`; expected NORMAL, REVERSE or HIGHLIGHT"),
        )),
    }
}

fn split_keyword(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(index) => (&line[..index], line[index..].trim_start()),
        None => (line, ""),
    }
}

/// `key: value` pairs, in any order, each key one of `allowed`. For
/// `commit`, a bare quoted string is its message (`commit "fix"`), as the
/// grammar's optional `msg:` allows.
fn attributes<'a>(
    rest: &str,
    line: usize,
    statement: &str,
    allowed: &[&'a str],
) -> Result<Vec<(&'a str, String)>, Error> {
    let expected = allowed
        .iter()
        .map(|key| format!("`{key}:`"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut pairs = Vec::new();
    let mut remaining = rest.trim();
    while !remaining.is_empty() {
        if remaining.starts_with(['"', '\'']) && allowed.contains(&"msg") {
            let (value, tail) = take_value(remaining, line)?;
            pairs.push(("msg", value));
            remaining = tail.trim_start();
            continue;
        }
        let word_end = remaining
            .find(|c: char| c == ':' || c.is_whitespace())
            .unwrap_or(remaining.len());
        let key = &remaining[..word_end];
        let after = remaining[word_end..].trim_start();
        let Some(after) = after.strip_prefix(':') else {
            return Err(Error::new(
                line,
                format!("unexpected `{remaining}` in `{statement}`; expected {expected}"),
            ));
        };
        let Some(&key) = allowed.iter().find(|allowed| **allowed == key) else {
            return Err(Error::new(
                line,
                format!("`{statement}` does not take `{key}:`; expected {expected}"),
            ));
        };
        let (value, tail) = take_value(after.trim_start(), line)?;
        if value.is_empty() && key != "tag" {
            return Err(Error::new(line, format!("`{key}:` needs a value")));
        }
        pairs.push((key, value));
        remaining = tail.trim_start();
    }
    Ok(pairs)
}

/// Take one value: a quoted string (`"…"` or `'…'`, with `\` escapes), or
/// everything up to the next whitespace.
fn take_value(input: &str, line: usize) -> Result<(String, &str), Error> {
    let input = input.trim_start();
    let Some(quote) = input.chars().next().filter(|c| *c == '"' || *c == '\'') else {
        let end = input.find(char::is_whitespace).unwrap_or(input.len());
        return Ok((input[..end].to_string(), &input[end..]));
    };
    let mut value = String::new();
    let mut chars = input.char_indices().skip(1);
    while let Some((index, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, escaped)) = chars.next() {
                    value.push(escaped);
                }
            }
            c if c == quote => return Ok((value, &input[index + 1..])),
            c => value.push(c),
        }
    }
    Err(Error::new(
        line,
        format!("unterminated string `{input}`; expected a closing {quote}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fails(source: &str) -> Error {
        parse(source).unwrap_err()
    }

    #[test]
    fn parses_a_linear_history() {
        let graph = parse("gitGraph\n  commit\n  commit\n").unwrap();
        assert_eq!(graph.branches.len(), 1);
        assert_eq!(graph.commits.len(), 2);
        assert_eq!(graph.commits[1].parents, vec![0]);
    }

    #[test]
    fn branch_creates_and_checks_out() {
        let graph = parse("gitGraph\n  commit\n  branch dev\n  commit\n").unwrap();
        assert_eq!(graph.branches[1].name, "dev");
        assert_eq!(graph.commits[1].branch, 1);
        assert_eq!(graph.commits[1].parents, vec![0]);
    }

    #[test]
    fn merge_records_both_parents() {
        let source = "gitGraph\n  commit\n  branch dev\n  commit\n  checkout main\n  merge dev\n";
        let graph = parse(source).unwrap();
        let merge = graph.commits.last().unwrap();
        assert_eq!(merge.parents, vec![0, 1]);
        assert_eq!(merge.branch, 0);
        assert_eq!(merge.kind, CommitType::Merge);
        assert_eq!(merge.origin, Origin::Merge);
        assert_eq!(merge.label, None);
    }

    #[test]
    fn parses_commit_attributes_in_any_order() {
        let graph =
            parse("gitGraph\n  commit tag: \"v1\" id: \"first\" type: HIGHLIGHT\n").unwrap();
        let commit = &graph.commits[0];
        assert_eq!(commit.id, "first");
        assert_eq!(commit.tags, vec!["v1"]);
        assert_eq!(commit.kind, CommitType::Highlight);
    }

    #[test]
    fn a_commit_may_carry_several_tags() {
        let graph = parse("gitGraph\n  commit tag: \"v1\" tag: \"stable\"\n").unwrap();
        assert_eq!(graph.commits[0].tags, vec!["v1", "stable"]);
    }

    #[test]
    fn msg_is_parsed_and_stored_with_or_without_the_keyword() {
        let graph =
            parse("gitGraph\n  commit msg: \"first: init\"\n  commit \"second\" id: \"b\"\n")
                .unwrap();
        assert_eq!(graph.commits[0].message.as_deref(), Some("first: init"));
        assert_eq!(graph.commits[1].message.as_deref(), Some("second"));
        assert_eq!(graph.commits[1].id, "b");
        // The label is the id, as mermaid draws it; the message is not drawn.
        assert_eq!(graph.commits[0].label.as_deref(), Some("c1"));
    }

    #[test]
    fn single_quotes_and_escapes_are_strings() {
        let graph = parse("gitGraph\n  commit id: 'a b' tag: \"say \\\"hi\\\"\"\n").unwrap();
        assert_eq!(graph.commits[0].id, "a b");
        assert_eq!(graph.commits[0].tags, vec!["say \"hi\""]);
        let error = fails("gitGraph\n  commit id: \"open\n");
        assert_eq!(error.line, 2);
        assert!(error.message.contains("unterminated"), "{error:?}");
    }

    #[test]
    fn auto_ids_are_assigned_when_absent() {
        let graph = parse("gitGraph\n  commit\n  commit id: \"c2\"\n  commit\n").unwrap();
        assert_eq!(graph.commits[0].id, "c1");
        assert_eq!(graph.commits[1].id, "c2");
        // An auto id never collides with an explicit one.
        assert_eq!(graph.commits[2].id, "c3");
    }

    #[test]
    fn duplicate_commit_ids_are_rejected() {
        let error = fails("gitGraph\n  commit id: \"a\"\n  commit id: \"a\"\n");
        assert_eq!(error.line, 3);
        assert!(error.message.contains("already exists"), "{error:?}");
    }

    #[test]
    fn switch_is_accepted_as_checkout() {
        let source = "gitGraph\n  commit\n  branch dev\n  switch main\n  commit\n";
        let graph = parse(source).unwrap();
        assert_eq!(graph.commits[1].branch, 0);
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let graph = parse("gitGraph\n\n  %% a note\n  commit %% trailing\n").unwrap();
        assert_eq!(graph.commits.len(), 1);
    }

    #[test]
    fn header_takes_an_optional_direction_and_colon() {
        for (header, direction) in [
            ("gitGraph", Direction::LR),
            ("gitGraph:", Direction::LR),
            ("gitGraph LR:", Direction::LR),
            ("gitGraph LR", Direction::LR),
            ("gitGraph TB:", Direction::TB),
            ("gitGraph TB", Direction::TB),
            ("gitGraph BT:", Direction::BT),
            ("gitGraph BT :", Direction::BT),
        ] {
            let graph = parse(&format!("{header}\n  commit\n")).unwrap();
            assert_eq!(graph.direction, direction, "{header}");
        }
        let error = fails("gitGraph RL:\n  commit\n");
        assert_eq!(error.line, 1);
        assert!(error.message.contains("`RL`"), "{error:?}");
        let error = fails("\ngitGraph TB: commit\n");
        assert_eq!(error.line, 2);
        assert!(error.message.contains("TB: commit"), "{error:?}");
    }

    #[test]
    fn title_and_accessibility_statements_are_stored() {
        let source = "gitGraph\n  title Release flow\n  accTitle: Releases\n  accDescr: How we ship\n  commit\n";
        let graph = parse(source).unwrap();
        assert_eq!(graph.title.as_deref(), Some("Release flow"));
        assert_eq!(graph.acc_title.as_deref(), Some("Releases"));
        assert_eq!(graph.acc_descr.as_deref(), Some("How we ship"));

        let graph = parse("gitGraph\n  accDescr {\n    two\n    lines\n  }\n  commit\n").unwrap();
        assert_eq!(graph.acc_descr.as_deref(), Some("two\nlines"));
        assert_eq!(graph.commits.len(), 1);

        let error = fails("gitGraph\n  accDescr {\n  commit\n");
        assert_eq!(error.line, 2);
        assert!(error.message.contains("never closed"), "{error:?}");
    }

    #[test]
    fn branch_order_is_parsed_and_sorts_lanes() {
        let source = "gitGraph\n  commit\n  branch late order: 3\n  branch plain\n  branch early order: 1\n  branch zero order: 0\n";
        let graph = parse(source).unwrap();
        assert_eq!(graph.branches[1].order, Some(3));
        assert_eq!(graph.branches[2].order, None);
        let names: Vec<&str> = graph
            .lane_order()
            .into_iter()
            .map(|index| graph.branches[index].name.as_str())
            .collect();
        // main (order 0) then the order-0 branch, unordered next, then 1, 3.
        assert_eq!(names, ["main", "zero", "plain", "early", "late"]);

        let error = fails("gitGraph\n  branch dev order: soon\n");
        assert_eq!(error.line, 2);
        assert!(error.message.contains("whole number"), "{error:?}");
        let error = fails("gitGraph\n  branch dev id: \"x\"\n");
        assert!(error.message.contains("does not take `id:`"), "{error:?}");
    }

    #[test]
    fn quoted_branch_names_may_contain_spaces() {
        let graph =
            parse("gitGraph\n  commit\n  branch \"my feature\"\n  commit\n  checkout main\n  merge \"my feature\"\n")
                .unwrap();
        assert_eq!(graph.branches[1].name, "my feature");
        assert_eq!(graph.commits.len(), 3);
    }

    #[test]
    fn merge_takes_id_tag_and_type() {
        let source = "gitGraph\n  commit\n  branch dev\n  commit\n  checkout main\n  merge dev id: \"m\" tag: \"v1\" type: REVERSE\n";
        let graph = parse(source).unwrap();
        let merge = graph.commits.last().unwrap();
        assert_eq!(merge.id, "m");
        assert_eq!(merge.label.as_deref(), Some("m"));
        assert_eq!(merge.tags, vec!["v1"]);
        assert_eq!(merge.kind, CommitType::Reverse);
        assert_eq!(merge.origin, Origin::Merge);

        let error = fails(&source.replace("type: REVERSE", "msg: \"x\""));
        assert_eq!(error.line, 6);
        assert!(error.message.contains("does not take `msg:`"), "{error:?}");
        let error = fails(&format!("{source}  commit id: \"m\"\n"));
        assert_eq!(error.line, 7);
    }

    #[test]
    fn merge_rejects_what_mermaid_rejects() {
        let error = fails("gitGraph\n  commit\n  merge main\n");
        assert!(error.message.contains("itself"), "{error:?}");
        let error = fails("gitGraph\n  branch dev\n  checkout main\n  merge dev\n");
        assert!(error.message.contains("has no commits"), "{error:?}");
        let error = fails("gitGraph\n  commit\n  branch dev\n  checkout main\n  merge dev\n");
        assert_eq!(error.line, 5);
        assert!(error.message.contains("no commits of its own"), "{error:?}");
        let error = fails("gitGraph\n  branch dev\n  commit\n  checkout main\n  merge dev\n");
        assert!(error.message.contains("into `main`"), "{error:?}");
        let error = fails("gitGraph\n  commit\n  merge nope\n");
        assert!(error.message.contains("unknown branch"), "{error:?}");
    }

    #[test]
    fn cherry_pick_adds_a_commit_with_the_source_as_second_parent() {
        let source = "gitGraph\n  commit id: \"a\"\n  branch dev\n  commit id: \"fix\"\n  checkout main\n  commit id: \"b\"\n  cherry-pick id: \"fix\"\n";
        let graph = parse(source).unwrap();
        let pick = graph.commits.last().unwrap();
        assert_eq!(pick.kind, CommitType::CherryPick);
        assert_eq!(pick.origin, Origin::CherryPick);
        assert_eq!(pick.branch, 0);
        assert_eq!(pick.parents, vec![2, 1]);
        assert_eq!(pick.label, None);
        assert_eq!(pick.tags, vec!["cherry-pick:fix"]);

        let tagged = parse(&source.replace("\"fix\"\n", "\"fix\" tag: \"backport\"\n")).unwrap();
        assert_eq!(tagged.commits.last().unwrap().tags, vec!["backport"]);
        let untagged = parse(&source.replace("\"fix\"\n", "\"fix\" tag: \"\"\n")).unwrap();
        assert!(untagged.commits.last().unwrap().tags.is_empty());
    }

    #[test]
    fn cherry_picking_a_merge_needs_one_of_its_parents() {
        let base = "gitGraph\n  commit id: \"a\"\n  branch dev\n  commit id: \"d\"\n  checkout main\n  commit id: \"b\"\n  merge dev id: \"m\"\n  checkout dev\n  commit id: \"r\"\n";
        let error = fails(&format!("{base}  cherry-pick id: \"m\"\n"));
        assert!(error.message.contains("is a merge"), "{error:?}");
        let error = fails(&format!("{base}  cherry-pick id: \"m\" parent: \"zzz\"\n"));
        assert!(error.message.contains("not a parent"), "{error:?}");
        let graph = parse(&format!("{base}  cherry-pick id: \"m\" parent: \"b\"\n")).unwrap();
        assert_eq!(
            graph.commits.last().unwrap().tags,
            vec!["cherry-pick:m|parent:b"]
        );
    }

    #[test]
    fn cherry_pick_rejects_what_mermaid_rejects() {
        let base = "gitGraph\n  commit id: \"a\"\n  branch dev\n  commit id: \"fix\"\n";
        let error = fails(&format!("{base}  cherry-pick tag: \"x\"\n"));
        assert_eq!(error.line, 5);
        assert!(error.message.contains("needs `id:`"), "{error:?}");
        let error = fails(&format!(
            "{base}  checkout main\n  cherry-pick id: \"nope\"\n"
        ));
        assert_eq!(error.line, 6);
        assert!(error.message.contains("unknown commit `nope`"), "{error:?}");
        let error = fails(&format!("{base}  cherry-pick id: \"fix\"\n"));
        assert!(
            error.message.contains("already on branch `dev`"),
            "{error:?}"
        );
        let error = fails(
            "gitGraph\n  branch dev\n  commit id: \"x\"\n  checkout main\n  cherry-pick id: \"x\"\n",
        );
        assert!(error.message.contains("onto `main`"), "{error:?}");
        let error = fails(&format!(
            "{base}  checkout main\n  cherry-pick id: \"fix\" type: HIGHLIGHT\n"
        ));
        assert!(error.message.contains("does not take `type:`"), "{error:?}");
    }

    #[test]
    fn unsupported_syntax_is_an_error_with_a_line_number() {
        let error = fails("gitGraph\n  rebase dev\n");
        assert_eq!(error.line, 2);
        assert!(error.message.contains("rebase"));

        let error = fails("gitGraph\n  commit foo\n");
        assert_eq!(error.line, 2);
        assert!(error.message.contains("unexpected `foo`"), "{error:?}");

        let error = fails("gitGraph\n  commit parent: \"x\"\n");
        assert!(
            error.message.contains("does not take `parent:`"),
            "{error:?}"
        );

        let error = fails("gitGraph\n  commit\n  checkout main extra\n");
        assert_eq!(error.line, 3);
        assert!(error.message.contains("one branch name"), "{error:?}");
    }

    #[test]
    fn unknown_branch_is_rejected() {
        let error = fails("gitGraph\n  commit\n  checkout nope\n");
        assert_eq!(error.line, 3);
        assert!(error.message.contains("unknown branch"));
    }
}
