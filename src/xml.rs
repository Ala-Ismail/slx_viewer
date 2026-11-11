use std::{collections::BTreeMap, sync::atomic::AtomicBool};

use quick_xml::{
    Reader, XmlVersion,
    encoding::Decoder,
    events::{BytesStart, Event},
};

use crate::{Limits, LoadError, check_cancelled};

pub(crate) struct Node {
    pub name: String,
    attrs: BTreeMap<String, String>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    pub fn attrs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.attrs
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |child| child.name == name)
    }
}

pub(crate) struct XmlBudget {
    elements: usize,
    depth: usize,
    maximum_elements: usize,
}

impl XmlBudget {
    pub fn new(limits: &Limits) -> Self {
        Self {
            elements: 0,
            depth: limits.xml_depth.min(128),
            maximum_elements: limits.xml_elements,
        }
    }

    pub fn parse(
        &mut self,
        data: &[u8],
        part: &str,
        cancelled: &AtomicBool,
    ) -> Result<Node, LoadError> {
        let xml_error = |error: &dyn std::fmt::Display| LoadError::format(part, error.to_string());
        let data = std::str::from_utf8(data).map_err(|e| xml_error(&e))?;
        if !data.chars().all(valid_xml_character) {
            return Err(LoadError::format(part, "invalid XML character"));
        }
        let mut reader = Reader::from_str(data);
        reader.config_mut().check_comments = true;
        let mut stack = Vec::<Node>::new();
        let mut root = None;
        loop {
            check_cancelled(cancelled)?;
            let offset = reader.buffer_position();
            match reader.read_event().map_err(|e| xml_error(&e))? {
                Event::Start(start) => {
                    stack.push(self.node(&start, reader.decoder(), stack.len(), part)?);
                }
                Event::Empty(start) => {
                    let node = self.node(&start, reader.decoder(), stack.len(), part)?;
                    attach(node, &mut stack, &mut root, part)?;
                }
                Event::End(_) => {
                    let node = stack
                        .pop()
                        .ok_or_else(|| LoadError::format(part, "unexpected closing tag"))?;
                    attach(node, &mut stack, &mut root, part)?;
                }
                Event::Text(text) => {
                    let text = text.xml10_content().map_err(|e| xml_error(&e))?;
                    if let Some(node) = stack.last_mut() {
                        node.text.push_str(&text);
                    } else if !text.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
                        return Err(LoadError::format(part, "text outside the root element"));
                    }
                }
                Event::CData(text) => {
                    let node = stack
                        .last_mut()
                        .ok_or_else(|| LoadError::format(part, "CDATA outside the root element"))?;
                    node.text
                        .push_str(&text.xml10_content().map_err(|e| xml_error(&e))?);
                }
                Event::GeneralRef(reference) => {
                    let node = stack.last_mut().ok_or_else(|| {
                        LoadError::format(part, "entity outside the root element")
                    })?;
                    let value = reference.decode().map_err(|e| xml_error(&e))?;
                    if let Some(character) =
                        reference.resolve_char_ref().map_err(|e| xml_error(&e))?
                    {
                        if !valid_xml_character(character) {
                            return Err(LoadError::format(part, "invalid XML character reference"));
                        }
                        node.text.push(character);
                    } else if let Some(text) = quick_xml::escape::resolve_xml_entity(&value) {
                        node.text.push_str(text);
                    } else {
                        return Err(LoadError::format(
                            part,
                            "only predefined XML entities are supported",
                        ));
                    }
                }
                Event::Decl(declaration) => {
                    if offset != 0 {
                        return Err(LoadError::format(part, "unexpected XML declaration"));
                    }
                    if declaration.version().map_err(|e| xml_error(&e))?.as_ref() != b"1.0" {
                        return Err(LoadError::format(part, "expected XML version 1.0"));
                    }
                    if let Some(encoding) = declaration.encoding() {
                        let encoding = encoding.map_err(|e| xml_error(&e))?;
                        if !encoding.eq_ignore_ascii_case(b"UTF-8") {
                            return Err(LoadError::format(part, "expected UTF-8 XML"));
                        }
                    }
                }
                Event::DocType(_) => {
                    return Err(LoadError::format(
                        part,
                        "XML document types and custom entities are forbidden",
                    ));
                }
                Event::Eof => break,
                Event::Comment(_) | Event::PI(_) => {}
            }
        }
        if !stack.is_empty() {
            return Err(LoadError::format(part, "unclosed XML element"));
        }
        root.ok_or_else(|| LoadError::format(part, "missing XML root"))
    }

    fn node(
        &mut self,
        start: &BytesStart<'_>,
        decoder: Decoder,
        depth: usize,
        part: &str,
    ) -> Result<Node, LoadError> {
        if depth >= self.depth {
            return Err(LoadError::limit("XML depth", self.depth));
        }
        if self.elements >= self.maximum_elements {
            return Err(LoadError::limit("XML element count", self.maximum_elements));
        }
        self.elements += 1;
        let xml_error = |error: &dyn std::fmt::Display| LoadError::format(part, error.to_string());
        let mut attrs = BTreeMap::new();
        for attr in start.attributes() {
            let attr = attr.map_err(|e| xml_error(&e))?;
            let key = std::str::from_utf8(attr.key.as_ref()).map_err(|e| xml_error(&e))?;
            let value = attr
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                .map_err(|e| xml_error(&e))?;
            if !value.chars().all(valid_xml_character) {
                return Err(LoadError::format(part, "invalid XML attribute character"));
            }
            attrs.insert(key.to_owned(), value.into_owned());
        }
        let name = start.name();
        let name = std::str::from_utf8(name.as_ref()).map_err(|e| xml_error(&e))?;
        Ok(Node {
            name: name.into(),
            attrs,
            children: Vec::new(),
            text: String::new(),
        })
    }
}

fn valid_xml_character(character: char) -> bool {
    matches!(character, '\t' | '\r' | '\n' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn attach(
    node: Node,
    stack: &mut [Node],
    root: &mut Option<Node>,
    part: &str,
) -> Result<(), LoadError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else if root.replace(node).is_some() {
        return Err(LoadError::format(part, "multiple XML root elements"));
    }
    Ok(())
}
