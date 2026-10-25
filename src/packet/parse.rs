//! The packet grammar: `packet/packet.langium` upstream.

use crate::Error;
use crate::meta::{self, Meta};

/// Bits per row, as upstream's default `bitsPerRow`.
pub const BITS_PER_ROW: u32 = 32;
/// More bits than any real header; guards against `0-4000000000`.
const MAX_BITS: u32 = BITS_PER_ROW * 1000;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Packet {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub fields: Vec<Field>,
}

/// One field, bits `start..=end`.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub start: u32,
    pub end: u32,
    pub label: String,
}

pub fn parse(source: &str) -> Result<Packet, Error> {
    let lines = meta::lines(source);
    meta::header(&lines, &["packet-beta", "packet"])?;
    let mut packet = Packet::default();
    let mut next = 0u32;
    let mut at = 1;
    while at < lines.len() {
        match meta::meta(&lines, &mut at)? {
            Some(Meta::Title(t)) => packet.title = Some(t),
            Some(Meta::AccTitle(t)) => packet.acc_title = Some(t),
            Some(Meta::AccDescr(t)) => packet.acc_descr = Some(t),
            None => {
                let (number, line) = lines[at];
                let field = field(line.trim(), number, next)?;
                next = field.end + 1;
                packet.fields.push(field);
                at += 1;
            }
        }
    }
    Ok(packet)
}

/// `0-15: "Source Port"`, `106: "URG"` or `+16: "Source Port"`.
fn field(line: &str, number: usize, next: u32) -> Result<Field, Error> {
    let (range, rest) = line.split_once(':').ok_or_else(|| {
        Error::new(
            number,
            format!("expected `start-end: \"label\"` or `+bits: \"label\"`, found `{line}`"),
        )
    })?;
    let range = range.trim();
    let int = |s: &str| -> Result<u32, Error> {
        s.trim()
            .parse::<u32>()
            .map_err(|_| Error::new(number, format!("`{}` is not a bit number", s.trim())))
    };
    let (start, end) = if let Some(bits) = range.strip_prefix('+') {
        let bits = int(bits)?;
        if bits == 0 {
            return Err(Error::new(number, "a field cannot have zero bits"));
        }
        (next, next.saturating_add(bits - 1))
    } else if let Some((a, b)) = range.split_once('-') {
        (int(a)?, int(b)?)
    } else {
        let a = int(range)?;
        (a, a)
    };
    if end < start {
        return Err(Error::new(
            number,
            format!("field {start}-{end} is invalid: the end must not be before the start"),
        ));
    }
    if start != next {
        return Err(Error::new(
            number,
            format!("field {start}-{end} is not contiguous: it should start at bit {next}"),
        ));
    }
    if end >= MAX_BITS {
        return Err(Error::new(
            number,
            format!("field {start}-{end} runs past bit {MAX_BITS}, more than mmd draws"),
        ));
    }
    let rest = rest.trim();
    let (label, after) = meta::quoted(rest)
        .ok_or_else(|| Error::new(number, format!("expected a quoted label, found `{rest}`")))?;
    if !after.trim().is_empty() {
        return Err(Error::new(
            number,
            format!("unexpected `{}` after the label", after.trim()),
        ));
    }
    Ok(Field {
        start,
        end,
        label: label.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_single_bits_and_counts() {
        let p =
            parse("packet-beta\ntitle UDP\n0-15: \"Src\"\n16: 'Flag'\n+15: \"Rest\"\n").unwrap();
        assert_eq!(p.title.as_deref(), Some("UDP"));
        let spans: Vec<(u32, u32)> = p.fields.iter().map(|f| (f.start, f.end)).collect();
        assert_eq!(spans, vec![(0, 15), (16, 16), (17, 31)]);
        assert_eq!(p.fields[1].label, "Flag");
    }

    #[test]
    fn mistakes_name_their_line() {
        for (source, line, needle) in [
            ("packet\n0-7: \"a\"\n9-10: \"b\"\n", 3, "contiguous"),
            ("packet\n7-0: \"a\"\n", 2, "invalid"),
            ("packet\n+0: \"a\"\n", 2, "zero"),
            ("packet\n0-7: a\n", 2, "quoted"),
            ("packet\n0-7 \"a\"\n", 2, "expected"),
            ("packet\n0-7: \"a\" b\n", 2, "unexpected"),
            ("packet\nx-7: \"a\"\n", 2, "bit number"),
            ("packet extra\n", 1, "unexpected"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
