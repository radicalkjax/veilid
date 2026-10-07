use super::*;

// Minimal well-formed-subset XML: elements, text, entities; attributes and namespaces ignored
#[derive(Clone, Debug)]
pub(crate) struct Element {
    pub name: String,
    pub children: Vec<Element>,
    pub text: String,
}

impl Element {
    pub fn child(&self, local_name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == local_name)
    }

    pub fn children_named<'a>(&'a self, local_name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == local_name)
    }

    pub fn child_text(&self, local_name: &str) -> Option<&str> {
        self.child(local_name).map(|c| c.text.trim())
    }
}

fn local_name(tag: &str) -> String {
    let name = tag.split([' ', '\t', '\r', '\n']).next().unwrap_or(tag);
    name.rsplit(':').next().unwrap_or(name).to_owned()
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

pub(crate) fn parse(input: &str) -> Result<Element, IgdError> {
    let mut stack: Vec<Element> = Vec::new();
    let mut rest = input;

    while let Some(lt) = rest.find('<') {
        let text = &rest[..lt];
        if let Some(top) = stack.last_mut() {
            if !text.trim().is_empty() {
                top.text.push_str(&decode_entities(text.trim()));
            }
        }
        rest = &rest[lt + 1..];

        if let Some(r) = rest.strip_prefix("?") {
            // processing instruction
            let end = r
                .find("?>")
                .ok_or_else(|| IgdError::parse("unterminated pi"))?;
            rest = &r[end + 2..];
        } else if let Some(r) = rest.strip_prefix("!--") {
            let end = r
                .find("-->")
                .ok_or_else(|| IgdError::parse("unterminated comment"))?;
            rest = &r[end + 3..];
        } else if let Some(r) = rest.strip_prefix("![CDATA[") {
            let end = r
                .find("]]>")
                .ok_or_else(|| IgdError::parse("unterminated cdata"))?;
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&r[..end]);
            }
            rest = &r[end + 3..];
        } else if let Some(r) = rest.strip_prefix("!") {
            // doctype or other declaration
            let end = r
                .find('>')
                .ok_or_else(|| IgdError::parse("unterminated decl"))?;
            rest = &r[end + 1..];
        } else if let Some(r) = rest.strip_prefix("/") {
            // closing tag
            let end = r
                .find('>')
                .ok_or_else(|| IgdError::parse("unterminated close tag"))?;
            let name = local_name(r[..end].trim());
            rest = &r[end + 1..];
            let elem = stack
                .pop()
                .ok_or_else(|| IgdError::parse("unbalanced close tag"))?;
            if elem.name != name {
                return Err(IgdError::parse("mismatched close tag"));
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(elem),
                None => return Ok(elem),
            }
        } else {
            // open or self-closing tag; skip attributes, respecting quotes
            let bytes = rest.as_bytes();
            let mut i = 0;
            let mut quote = 0u8;
            while i < bytes.len() {
                let b = bytes[i];
                if quote != 0 {
                    if b == quote {
                        quote = 0;
                    }
                } else if b == b'"' || b == b'\'' {
                    quote = b;
                } else if b == b'>' {
                    break;
                }
                i += 1;
            }
            if i >= bytes.len() {
                return Err(IgdError::parse("unterminated tag"));
            }
            let self_closing = rest[..i].ends_with('/');
            let tag = rest[..i].trim_end_matches('/').trim();
            let elem = Element {
                name: local_name(tag),
                children: Vec::new(),
                text: String::new(),
            };
            rest = &rest[i + 1..];
            if self_closing {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(elem),
                    None => return Ok(elem),
                }
            } else {
                stack.push(elem);
            }
        }
    }
    Err(IgdError::parse("no root element"))
}
