use scanner::{Marker, ScanError, TScalarStyle, TokenType};
use parser::*;
use yaml::{Yaml, Hash};
use settings::{YamlSettings, YamlStandardSettings};

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

struct NodeWithAnchor {
    node: Yaml,
    anchor: Option<AnchorId>,
}

impl NodeWithAnchor {
    fn new( node: Yaml, anchor: Option<AnchorId> ) -> Self {
        Self {
            node,
            anchor,
        }
    }
}

pub struct YamlLoader<TS: YamlSettings = YamlStandardSettings> {
    settings: TS,
    docs: Vec<Yaml>,
    // states
    doc_stack: Vec<NodeWithAnchor>,
    key_stack: Vec<Yaml>,
    anchor_map: BTreeMap<AnchorId, Yaml>,
}

impl<TS: YamlSettings> MarkedEventReceiver for YamlLoader<TS> {
    fn on_event(&mut self, ev: Event, _: Marker) {
        // println!("EV {:?}", ev);
        match ev {
            Event::DocumentStart => {
                // do nothing
            }
            Event::DocumentEnd => {
                match self.doc_stack.len() {
                    // empty document
                    0 => self.docs.push(Yaml::BadValue),
                    1 => self.docs.push(self.doc_stack.pop().unwrap().node),
                    _ => unreachable!(),
                }
            }
            Event::SequenceStart(anchor) => {
                self.doc_stack.push(NodeWithAnchor::new(Yaml::Array(Vec::new()), anchor));
            }
            Event::SequenceEnd => {
                let node = self.doc_stack.pop().unwrap();
                self.insert_new_node(node);
            }
            Event::MappingStart(anchor) => {
                self.doc_stack.push(NodeWithAnchor::new(Yaml::Hash(Hash::new()), anchor));
                self.key_stack.push(Yaml::BadValue);
            }
            Event::MappingEnd => {
                self.key_stack.pop().unwrap();
                let node = self.doc_stack.pop().unwrap();
                self.insert_new_node(node);
            }
            Event::Scalar{value, style, anchor, tag} => {
                let node = if style != TScalarStyle::Plain {
                    Yaml::String(value)
                } else if let Some(TokenType::Tag(ref handle, ref suffix)) = tag {
                    // XXX tag:yaml.org,2002:
                    if handle == "!!" {
                        match suffix.as_ref() {
                            "bool" => {
                                // "true" or "false"
                                match value.parse::<bool>() {
                                    Err(_) => Yaml::BadValue,
                                    Ok(v) => Yaml::Boolean(v),
                                }
                            }
                            "int" => match value.parse::<i64>() {
                                Err(_) => Yaml::BadValue,
                                Ok(v) => Yaml::Integer(v),
                            },
                            "float" => match parse_f64(&value) {
                                Some(_) => Yaml::Real(value),
                                None => Yaml::BadValue,
                            },
                            "null" => match value.as_ref() {
                                "~" | "null" => Yaml::Null,
                                _ => Yaml::BadValue,
                            },
                            _ => Yaml::String(value),
                        }
                    } else {
                        Yaml::String(value)
                    }
                } else {
                    // Datatype is not specified, or unrecognized
                    Yaml::from_str(&value)
                };

                self.insert_new_node(NodeWithAnchor::new(node, anchor));
            }
            Event::Alias(anchor_id) => {
                let n = if self.settings.is_aliases_allowed() {
                    match self.anchor_map.get(&anchor_id) {
                        Some(v) => v.clone(),
                        None => Yaml::BadValue,
                    }
                } else {
                    Yaml::BadValue
                };
                self.insert_new_node(NodeWithAnchor::new(n, None));
            }
            _ => { /* ignore */ }
        }
        // println!("DOC {:?}", self.doc_stack);
    }
}

impl<TS: YamlSettings> YamlLoader<TS> {
    fn insert_new_node(&mut self, node: NodeWithAnchor) {
        // valid anchor id starts from 1
        if let Some(anchor_id) = node.anchor {
            self.anchor_map.insert(anchor_id, node.node.clone());
        }
        if self.doc_stack.is_empty() {
            self.doc_stack.push(node);
        } else {
            let parent = self.doc_stack.last_mut().unwrap();
            match *parent {
                NodeWithAnchor{node: Yaml::Array(ref mut v), anchor: _} => v.push(node.node),
                NodeWithAnchor{node: Yaml::Hash(ref mut h), anchor: _} => {
                    let cur_key = self.key_stack.last_mut().unwrap();
                    // current node is a key
                    if cur_key.is_badvalue() {
                        *cur_key = node.node;
                    // current node is a value
                    } else {
                        let mut newkey = Yaml::BadValue;
                        mem::swap(&mut newkey, cur_key);
                        h.insert(newkey, node.node);
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn load_from_str(source: &str, settings: &TS) -> Result<Vec<Yaml>, ScanError> {
        let mut loader = YamlLoader {
            settings: settings.clone(),
            docs: Vec::new(),
            doc_stack: Vec::new(),
            key_stack: Vec::new(),
            anchor_map: BTreeMap::new(),
        };
        let mut parser = Parser::new(source.chars(), &loader.settings);
        parser.load(&mut loader, true)?;
        Ok(loader.docs)
    }
}

pub fn yaml_load_from_str(source: &str) -> Result<Vec<Yaml>, ScanError> {
    let settings = YamlStandardSettings::new();
    YamlLoader::load_from_str(source, &settings)
}

pub fn yaml_load_from_str_safe(source: &str) -> Result<Vec<Yaml>, ScanError> {
    let settings = YamlStandardSettings::new_safe();
    YamlLoader::load_from_str(source, &settings)
}

pub fn yaml_load_doc_from_str(source: &str) -> Option<Yaml> {
    let mut docs = yaml_load_from_str(source).ok()?;
    if docs.len() != 1 {
        return None;
    }
    let doc = docs.swap_remove(0);
    Some(doc)
}

pub fn yaml_load_doc_from_str_safe(source: &str) -> Option<Yaml> {
    let mut docs = yaml_load_from_str_safe(source).ok()?;
    if docs.len() != 1 {
        return None;
    }
    let doc = docs.swap_remove(0);
    Some(doc)
}

