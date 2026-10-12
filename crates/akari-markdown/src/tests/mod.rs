mod bounds;
mod corpus;
mod outline;
mod text;

use crate::{Document, InlineKind, Span};

pub(crate) fn spans(doc: &Document) -> Vec<(&'static str, Span)> {
    let mut out = Vec::new();
    for block in doc.blocks() {
        out.push(("block", block.source));
        if let crate::BlockKind::CodeBlock { language: Some(l) } = block.kind {
            out.push(("language", l));
        }
        for inline in block.inlines.iter() {
            out.push(("inline source", inline.source));
            match inline.kind {
                InlineKind::Text(s) | InlineKind::Code(s) => out.push(("display", s)),
                InlineKind::Command { name, .. } | InlineKind::CustomEmoji { name, .. } => {
                    out.push(("name", name));
                }
                _ => {}
            }
        }
    }
    out
}
