//! gitGraph parsing.
//!
//! Grammar subset per `DESIGN.md`, checked against mermaid's
//! `packages/parser/src/language/gitGraph/gitGraph.langium`.

use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommitType {
    #[default]
    Normal,
    Reverse,
    Highlight,
}

#[derive(Debug, Clone)]
pub struct Commit {
    pub id: String,
    pub tag: Option<String>,
    pub kind: CommitType,
    /// Index into [`GitGraph::branches`].
    pub branch: usize,
    /// Earlier commits this one follows. A merge has two.
    pub parents: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct GitGraph {
    pub branches: Vec<Branch>,
    pub commits: Vec<Commit>,
}

/// Parse a gitGraph source.
pub fn parse(source: &str) -> Result<GitGraph, Error> {
    let mut graph = GitGraph::default();
    graph.branches.push(Branch {
        name: "main".to_string(),
    });

    let mut heads: Vec<Option<usize>> = vec![None];
    let mut current = 0usize;
    let mut seen_header = false;
    let mut auto_id = 0usize;

    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if !seen_header {
            if line == "gitGraph" || line == "gitGraph:" || line.starts_with("gitGraph") {
                seen_header = true;
                continue;
            }
            return Err(Error::new(line_number, "expected `gitGraph`"));
        }

        let (keyword, rest) = split_keyword(line);
        match keyword {
            "commit" => {
                let (id, tag, kind) = parse_commit_args(rest, line_number, &mut auto_id)?;
                let parents = heads[current].map(|head| vec![head]).unwrap_or_default();
                graph.commits.push(Commit {
                    id,
                    tag,
                    kind,
                    branch: current,
                    parents,
                });
                heads[current] = Some(graph.commits.len() - 1);
            }
            "branch" => {
                let name = branch_name(rest, line_number, "branch")?;
                if find_branch(&graph, &name).is_some() {
                    return Err(Error::new(
                        line_number,
                        format!("branch `{name}` already exists"),
                    ));
                }
                graph.branches.push(Branch { name });
                // A new branch starts at the current head and is checked out.
                heads.push(heads[current]);
                current = graph.branches.len() - 1;
            }
            "checkout" | "switch" => {
                let name = branch_name(rest, line_number, keyword)?;
                current = find_branch(&graph, &name)
                    .ok_or_else(|| Error::new(line_number, format!("unknown branch `{name}`")))?;
            }
            "merge" => {
                let name = branch_name(rest, line_number, "merge")?;
                let source_branch = find_branch(&graph, &name)
                    .ok_or_else(|| Error::new(line_number, format!("unknown branch `{name}`")))?;
                if source_branch == current {
                    return Err(Error::new(
                        line_number,
                        format!("cannot merge branch `{name}` into itself"),
                    ));
                }
                let mut parents = Vec::new();
                if let Some(head) = heads[current] {
                    parents.push(head);
                }
                let other = heads[source_branch].ok_or_else(|| {
                    Error::new(
                        line_number,
                        format!("branch `{name}` has no commits to merge"),
                    )
                })?;
                parents.push(other);
                graph.commits.push(Commit {
                    id: format!("merge {name}"),
                    tag: None,
                    kind: CommitType::Normal,
                    branch: current,
                    parents,
                });
                heads[current] = Some(graph.commits.len() - 1);
            }
            "cherry-pick" => {
                return Err(Error::new(
                    line_number,
                    "`cherry-pick` is not supported (see DESIGN.md)",
                ));
            }
            other => {
                return Err(Error::new(
                    line_number,
                    format!("unexpected `{other}`; expected commit, branch, checkout or merge"),
                ));
            }
        }
    }

    if !seen_header {
        return Err(Error::new(1, "expected `gitGraph`"));
    }
    Ok(graph)
}

fn find_branch(graph: &GitGraph, name: &str) -> Option<usize> {
    graph.branches.iter().position(|branch| branch.name == name)
}

fn split_keyword(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(index) => (&line[..index], line[index..].trim_start()),
        None => (line, ""),
    }
}

fn branch_name(rest: &str, line: usize, keyword: &str) -> Result<String, Error> {
    let name = unquote(rest.trim());
    if name.is_empty() {
        return Err(Error::new(line, format!("`{keyword}` needs a branch name")));
    }
    if name.contains(char::is_whitespace) {
        return Err(Error::new(
            line,
            format!("`{keyword}` takes one branch name, got `{name}`"),
        ));
    }
    Ok(name.to_string())
}

/// `id: "x"`, `tag: "v1"` and `type: HIGHLIGHT`, in any order.
fn parse_commit_args(
    rest: &str,
    line: usize,
    auto_id: &mut usize,
) -> Result<(String, Option<String>, CommitType), Error> {
    let mut id = None;
    let mut tag = None;
    let mut kind = CommitType::Normal;

    let mut remaining = rest.trim();
    while !remaining.is_empty() {
        let colon = remaining.find(':').ok_or_else(|| {
            Error::new(
                line,
                format!("expected `id:`, `tag:` or `type:` in `{remaining}`"),
            )
        })?;
        let key = remaining[..colon].trim();
        let after = remaining[colon + 1..].trim_start();
        let (value, tail) = take_value(after);
        remaining = tail.trim_start();

        match key {
            "id" => id = Some(unquote(value).to_string()),
            "tag" => tag = Some(unquote(value).to_string()),
            "type" => {
                kind = match unquote(value) {
                    "NORMAL" => CommitType::Normal,
                    "REVERSE" => CommitType::Reverse,
                    "HIGHLIGHT" => CommitType::Highlight,
                    other => {
                        return Err(Error::new(
                            line,
                            format!(
                                "unknown commit type `{other}`; expected NORMAL, REVERSE or HIGHLIGHT"
                            ),
                        ));
                    }
                }
            }
            "msg" => {
                return Err(Error::new(line, "`msg:` is not supported (see DESIGN.md)"));
            }
            other => {
                return Err(Error::new(
                    line,
                    format!("unknown commit attribute `{other}:`"),
                ));
            }
        }
    }

    let id = id.unwrap_or_else(|| {
        *auto_id += 1;
        format!("c{auto_id}")
    });
    Ok((id, tag, kind))
}

/// Take one value: a quoted string, or everything up to the next whitespace.
fn take_value(input: &str) -> (&str, &str) {
    if let Some(rest) = input.strip_prefix('"') {
        match rest.find('"') {
            Some(end) => (&rest[..end], &rest[end + 1..]),
            None => (rest, ""),
        }
    } else {
        match input.find(char::is_whitespace) {
            Some(index) => (&input[..index], &input[index..]),
            None => (input, ""),
        }
    }
}

fn unquote(value: &str) -> &str {
    let trimmed = value.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(merge.parents.len(), 2);
        assert_eq!(merge.branch, 0);
    }

    #[test]
    fn parses_commit_attributes_in_any_order() {
        let graph =
            parse("gitGraph\n  commit tag: \"v1\" id: \"first\" type: HIGHLIGHT\n").unwrap();
        let commit = &graph.commits[0];
        assert_eq!(commit.id, "first");
        assert_eq!(commit.tag.as_deref(), Some("v1"));
        assert_eq!(commit.kind, CommitType::Highlight);
    }

    #[test]
    fn auto_ids_are_assigned_when_absent() {
        let graph = parse("gitGraph\n  commit\n  commit\n").unwrap();
        assert_eq!(graph.commits[0].id, "c1");
        assert_eq!(graph.commits[1].id, "c2");
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
    fn unsupported_syntax_is_an_error_with_a_line_number() {
        let error = parse("gitGraph\n  commit\n  cherry-pick id: \"x\"\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("cherry-pick"));

        let error = parse("gitGraph\n  rebase dev\n").unwrap_err();
        assert_eq!(error.line, 2);
    }

    #[test]
    fn unknown_branch_is_rejected() {
        let error = parse("gitGraph\n  commit\n  checkout nope\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("unknown branch"));
    }

    #[test]
    fn merging_a_branch_with_no_commits_is_rejected() {
        let error = parse("gitGraph\n  branch dev\n  checkout main\n  merge dev\n").unwrap_err();
        assert!(error.message.contains("no commits"), "{error:?}");
    }

    #[test]
    fn merging_a_branch_into_itself_is_rejected() {
        let error = parse("gitGraph\n  commit\n  merge main\n").unwrap_err();
        assert!(error.message.contains("itself"), "{error:?}");
    }
}
