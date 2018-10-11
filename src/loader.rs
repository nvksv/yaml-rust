use scanner::{Marker, ScanError, TScalarStyle, TokenType};
use parser::*;
use yaml::{Yaml, Hash};
use settings::{YamlSettings, YamlStandardSettings};
use builder::{YamlBuilder, YamlStandardBuilder, YamlNodeKind};

use std::mem;
use std::collections::BTreeMap;
use std::f64;


// parse f64 as Core schema
// See: https://github.com/chyh1990/yaml-rust/issues/51
pub fn parse_f64(v: &str) -> Option<f64> {
    match v {
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
        ".nan" | "NaN" | ".NAN" => Some(f64::NAN),
        _ => v.parse::<f64>().ok(),
    }
}

struct NodeWithAnchor<TB> where TB: YamlBuilder {
    node: TB::NodeHandle,
    anchor: Option<AnchorId>,
}

impl<TB> NodeWithAnchor<TB> where TB: YamlBuilder {
    fn new( node: TB::NodeHandle, anchor: Option<AnchorId> ) -> Self {
        Self {
            node,
            anchor,
        }
    }
}

pub struct YamlLoader<TS = YamlStandardSettings, TB = YamlStandardBuilder<TS>> where TS: YamlSettings, TB: YamlBuilder {
    settings: TS,
    builder: TB,
    // states
    doc_stack: Vec<NodeWithAnchor<TB>>,
    key_stack: Vec<TB::NodeHandle>,
    anchor_map: BTreeMap<AnchorId, TB::NodeHandle>,
    doc: Option<TB::NodeHandle>,
}

impl<TS, TB> MarkedEventReceiver for YamlLoader<TS, TB> where TS: YamlSettings, TB: YamlBuilder {
    fn on_event(&mut self, ev: Event, marker: Marker) {
        // println!("EV {:?}", ev);
        match ev {
            Event::DocumentStart => {
                self.doc = Some(self.builder.new_document(marker));
            }
            Event::DocumentEnd => {
                let content = match self.doc_stack.len() {
                    // empty document                    
                    0 => self.builder.new_badvalue(marker), 
                    // document content node
                    1 => self.doc_stack.pop().unwrap().node,
                    _ => unreachable!(),
                };
                if let Some(doc) = self.doc {
                    self.builder.close_document(doc, content);
                } else {
                    unreachable!()
                };
                self.doc = None;
            }
            Event::SequenceStart(anchor) => {
                self.doc_stack
                    .push(NodeWithAnchor::new(self.builder.new_sequence(marker), anchor));
            }
            Event::SequenceEnd => {
                let node = self.doc_stack.pop().unwrap();
                self.builder.close_sequence(node.node);
                self.insert_new_node(node, marker);
            }
            Event::MappingStart(anchor) => {
                self.doc_stack
                    .push(NodeWithAnchor::new(self.builder.new_mapping(marker), anchor));
                self.key_stack.push(self.builder.new_badvalue(marker));
            }
            Event::MappingEnd => {
                self.key_stack.pop().unwrap();
                let node = self.doc_stack.pop().unwrap();
                self.builder.close_mapping(node.node);
                self.insert_new_node(node, marker);
            }
            Event::Scalar{value, style, anchor, tag} => {
                let node = if style != TScalarStyle::Plain {
                    self.builder.new_string(value, marker)
                } else if let Some(TokenType::Tag(ref handle, ref suffix)) = tag {
                    // XXX tag:yaml.org,2002:
                    if handle == "!!" {
                        match suffix.as_ref() {
                            "bool" => {
                                // "true" or "false"
                                match value.parse::<bool>() {
                                    Err(_) => self.builder.new_badvalue(marker),
                                    Ok(v) => self.builder.new_bool(v, marker),
                                }
                            }
                            "int" => match value.parse::<i64>() {
                                Err(_) => self.builder.new_badvalue(marker),
                                Ok(v) => self.builder.new_int(v, marker),
                            },
                            "float" => match parse_f64(&value) {
                                Some(v) => self.builder.new_float(v, marker),
                                None => self.builder.new_badvalue(marker),
                            },
                            "null" => match value.as_ref() {
                                "~" | "null" => self.builder.new_null(marker),
                                _ => self.builder.new_badvalue(marker),
                            },
                            _ => self.builder.new_string(value, marker),
                        }
                    } else {
                        self.builder.new_string(value, marker)
                    }
                } else {
                    // Datatype is not specified, or unrecognized
                    self.from_str(&value, marker)
                };

                self.insert_new_node(NodeWithAnchor::new(node, anchor), marker);
            }
            Event::Alias(anchor_id) => {
                let n = if self.settings.is_aliases_allowed() {
                    match self.anchor_map.get(&anchor_id) {
                        Some(&v) => self.builder.clone_node(v),
                        None => self.builder.new_badvalue(marker),
                    }
                } else {
                    self.builder.new_badvalue(marker)
                };
                self.insert_new_node(NodeWithAnchor::new(n, None), marker);
            }
            _ => { /* ignore */ }
        }
        // println!("DOC {:?}", self.doc_stack);
    }
}

impl<TS, TB> YamlLoader<TS, TB> where TS: YamlSettings, TB: YamlBuilder {
    
    fn new(settings: &TS, builder: &TB) -> Self {
        YamlLoader {
            settings: settings.clone(),
            builder: builder.clone(),
            doc_stack: Vec::new(),
            key_stack: Vec::new(),
            anchor_map: BTreeMap::new(),
            doc: None,
        }
    }

    pub fn from_str(&mut self, v: &str, marker: Marker) -> TB::NodeHandle {
        if v.starts_with("0x") {
            if let Ok(i) = i64::from_str_radix(&v[2..], 16) {
                return self.builder.new_int(i, marker);
            }
        } else if v.starts_with("0o") {
            if let Ok(i) = i64::from_str_radix(&v[2..], 8) {
                return self.builder.new_int(i, marker);
            }
        } else if v.starts_with('+') {
            if let Ok(i) = v[1..].parse::<i64>() {
                return self.builder.new_int(i, marker);
            }
        } else if let Ok(i) = v.parse::<i64>() {
            return self.builder.new_int(i, marker);
        } else if let Some(f) = parse_f64(v) {
            return self.builder.new_float(f, marker);
        }
        match v {
            "~" | "null" => self.builder.new_null(marker),
            "true" => self.builder.new_bool(true, marker),
            "false" => self.builder.new_bool(false, marker),
            _ => self.builder.new_string(v.to_owned(), marker),
        }
    }

    fn insert_new_node(&mut self, node: NodeWithAnchor<TB>, marker: Marker) {
        if self.settings.is_aliases_allowed() {
            if let Some(anchor_id) = node.anchor {
                self.anchor_map
                    .insert(anchor_id, self.builder.clone_node(node.node));
            }
        }
        if self.doc_stack.is_empty() {
            self.doc_stack.push(node);
        } else {
            let parent = self.doc_stack.last_mut().unwrap();
            match self.builder.get_node_kind(parent.node) {
                YamlNodeKind::Sequence => self.builder.add_to_sequence(parent.node, node.node),
                YamlNodeKind::Mapping => {
                    let cur_key = self.key_stack.last_mut().unwrap();
                    // current node is a key
                    if self.builder.is_badvalue(*cur_key) {
                        *cur_key = node.node;
                    // current node is a value
                    } else {
                        // current node is a value
                        let mut newkey = self.builder.new_badvalue(marker);
                        mem::swap(&mut newkey, cur_key);
                        self.builder.add_to_mapping(parent.node, newkey, node.node);
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn load_from_iter<T: Iterator<Item = char>>(&mut self, source: T) -> Result<(), ScanError> {
        let mut parser = Parser::new(source, &self.settings);
        parser.load(self, true)?;
        Ok(())
    }
}

pub fn yaml_load_from_str_with_settings<TS>(source: &str, settings: &TS) -> Result<Vec<Yaml>, ScanError> where TS: YamlSettings {
    let builder = YamlStandardBuilder::new(settings);
    let mut loader = YamlLoader::new(settings, &builder);
    loader.load_from_iter(source.chars())?;
    Ok(builder.into_documents())
}

pub fn yaml_load_from_str(source: &str) -> Result<Vec<Yaml>, ScanError> {
    let settings = YamlStandardSettings::new();
    yaml_load_from_str_with_settings(source, &settings)
}

pub fn yaml_load_from_str_safe(source: &str) -> Result<Vec<Yaml>, ScanError> {
    let settings = YamlStandardSettings::new_safe();
    yaml_load_from_str_with_settings(source, &settings)
}

fn get_one_doc(res: Result<Vec<Yaml>, ScanError>) -> Option<Yaml> {
    let mut docs = res.ok()?;
    if docs.len() != 1 {
        return None;
    }
    let doc = docs.swap_remove(0);
    Some(doc)
}

pub fn yaml_load_doc_from_str(source: &str) -> Option<Yaml> {
    get_one_doc(yaml_load_from_str(source))
}

pub fn yaml_load_doc_from_str_safe(source: &str) -> Option<Yaml> {
    get_one_doc(yaml_load_from_str_safe(source))
}

