use std::collections::BTreeMap;
use std::rc::Rc;
use std::cell::RefCell;

use yaml::{self, Int, Float, Bool, String, Yaml};
use scanner::Marker;
use settings::YamlSettings;

#[derive(Debug)]
pub enum YamlNodeKind {
    Sequence,
    Mapping,
    KeyValuePair,
    Scalar,
}

pub trait YamlBuilder: Clone {
    type NodeHandle: Copy;

    fn new_badvalue(&mut self, marker: Marker) -> Self::NodeHandle;
    fn new_null(&mut self, marker: Marker) -> Self::NodeHandle;
    fn new_sequence(&mut self, marker: Marker) -> Self::NodeHandle;
    fn new_mapping(&mut self, marker: Marker) -> Self::NodeHandle;
    fn new_float(&mut self, value: Float, marker: Marker) -> Self::NodeHandle;
    fn new_int(&mut self, value: Int, marker: Marker) -> Self::NodeHandle;
    fn new_string(&mut self, value: String, marker: Marker) -> Self::NodeHandle;
    fn new_bool(&mut self, value: Bool, marker: Marker) -> Self::NodeHandle;

    fn add_to_sequence(&mut self, sequence: Self::NodeHandle, item: Self::NodeHandle);
    fn close_sequence(&mut self, sequence: Self::NodeHandle);

    fn add_to_mapping(
        &mut self,
        mapping: Self::NodeHandle,
        key: Self::NodeHandle,
        item: Self::NodeHandle,
    );
    fn close_mapping(&mut self, mapping: Self::NodeHandle);

    fn new_document(&mut self, marker: Marker) -> Self::NodeHandle;
    fn close_document(&mut self, document: Self::NodeHandle, content: Self::NodeHandle);

    fn get_node_kind(&self, node: Self::NodeHandle) -> YamlNodeKind;
    fn clone_node(&mut self, node: Self::NodeHandle) -> Self::NodeHandle;
    fn is_badvalue(&self, node: Self::NodeHandle) -> bool;
}

type NodeHandle = usize;
type YamlMap = yaml::Hash;
type YamlSeq = yaml::Array;

#[derive(Clone)]
pub struct YamlStandardBuilder<TS> where TS: YamlSettings {
    v: Rc<RefCell<YamlStandardBuilderData<TS>>>,
}

struct YamlStandardBuilderData<TS> where TS: YamlSettings {
    _settings: TS,
    counter: NodeHandle,
    nodes: BTreeMap<NodeHandle, Yaml>,
    docs: Vec<Yaml>,
}

impl<TS> YamlStandardBuilderData<TS> where TS: YamlSettings {

    fn new(settings: &TS) -> Self {
        Self {
            _settings: settings.clone(),
            counter: 1,
            nodes: BTreeMap::new(),
            docs: Vec::new(),
        }
    }

    fn get_next_handle(&mut self) -> NodeHandle {
        self.counter += 1;
        self.counter
    }

    fn push_node(&mut self, node: Yaml) -> NodeHandle {
        let handle = self.get_next_handle();
        self.nodes.insert(handle, node);
        handle
    }

    fn get_node(&self, handle: NodeHandle) -> Option<&Yaml> {
        self.nodes.get(&handle)
    }

    fn get_node_mut(&mut self, handle: NodeHandle) -> Option<&mut Yaml> {
        self.nodes.get_mut(&handle)
    }

    fn take_node(&mut self, handle: NodeHandle) -> Option<Yaml> {
        self.nodes.remove(&handle)
    }

}

fn move_out_vec<T>(v: &mut Vec<T>) -> Vec<T> {
    v.drain(..).collect()
}

impl<TS> YamlStandardBuilder<TS> where TS: YamlSettings {
    
    pub fn new(settings: &TS) -> Self {
        let data = YamlStandardBuilderData::new(settings);
        Self {
            v: Rc::new(RefCell::new(data)),
        }
    }

    pub fn into_documents(self) -> Vec<Yaml> {
        move_out_vec(&mut self.v.borrow_mut().docs)
    }

}

impl<TS> YamlBuilder for YamlStandardBuilder<TS> where TS: YamlSettings {
    type NodeHandle = NodeHandle;

    fn new_badvalue(&mut self, _marker: Marker) -> NodeHandle {
        let node = Yaml::BadValue;
        self.v.borrow_mut().push_node(node)
    }

    fn new_null(&mut self, _marker: Marker) -> NodeHandle {
        let node = Yaml::Null;
        self.v.borrow_mut().push_node(node)
    }

    fn new_sequence(&mut self, _marker: Marker) -> NodeHandle {
        let node = Yaml::Array(YamlSeq::new());
        self.v.borrow_mut().push_node(node)
    }

    fn new_mapping(&mut self, _marker: Marker) -> NodeHandle {
        let node = Yaml::Hash(YamlMap::new());
        self.v.borrow_mut().push_node(node)
    }

    fn new_float(&mut self, value: Float, _marker: Marker) -> NodeHandle {
        let node = Yaml::Real(value.to_string());
        self.v.borrow_mut().push_node(node)
    }

    fn new_int(&mut self, value: i64, _marker: Marker) -> NodeHandle {
        let node = Yaml::Integer(value);
        self.v.borrow_mut().push_node(node)
    }

    fn new_string(&mut self, value: String, _marker: Marker) -> NodeHandle {
        let node = Yaml::String(value);
        self.v.borrow_mut().push_node(node)
    }

    fn new_bool(&mut self, value: bool, _marker: Marker) -> NodeHandle {
        let node = Yaml::Boolean(value);
        self.v.borrow_mut().push_node(node)
    }

    fn add_to_sequence(&mut self, h_sequence: NodeHandle, h_item: NodeHandle) {
        let mut dataref = self.v.borrow_mut();

        let item = dataref.take_node(h_item).unwrap();
        let sequence = dataref.get_node_mut(h_sequence).unwrap();

        match *sequence {
            Yaml::Array(ref mut v) => v.push(item),
            _ => unreachable!(),
        }
    }

    fn close_sequence(&mut self, _h_sequence: NodeHandle) {}

    fn add_to_mapping(
        &mut self,
        h_mapping: NodeHandle,
        h_key: NodeHandle,
        h_item: NodeHandle,
    ) {
        let mut dataref = self.v.borrow_mut();

        let key = dataref.take_node(h_key).unwrap();
        let item = dataref.take_node(h_item).unwrap();
        let mapping = dataref.get_node_mut(h_mapping).unwrap();

        match *mapping {
            Yaml::Hash(ref mut h) => h.insert(key, item),
            _ => unreachable!(),
        };
    }

    fn close_mapping(&mut self, _h_mapping: NodeHandle) {}

    fn new_document(&mut self, _marker: Marker) -> NodeHandle {
        self.v.borrow_mut().get_next_handle()
    }

    fn close_document(&mut self, _h_document: NodeHandle, h_content: NodeHandle) {
        let mut dataref = self.v.borrow_mut();

        let content = dataref.take_node(h_content).unwrap();
        dataref.docs.push(content);
    }

    fn get_node_kind(&self, h_node: NodeHandle) -> YamlNodeKind {
        let dataref = self.v.borrow();
        let node = dataref.get_node(h_node).unwrap();
        match *node {
            Yaml::Array(..) => YamlNodeKind::Sequence,
            Yaml::Hash(..) => YamlNodeKind::Mapping,
            _ => YamlNodeKind::Scalar,
        }
    }

    fn clone_node(&mut self, h_node: NodeHandle) -> NodeHandle {
        let mut dataref = self.v.borrow_mut();

        let node2 = dataref.get_node(h_node).unwrap().clone();
        dataref.push_node(node2)
    }

    fn is_badvalue(&self, h_node: NodeHandle) -> bool {
        let dataref = self.v.borrow();
        let node = dataref.get_node(h_node).unwrap();
        match *node {
            Yaml::BadValue => true,
            _ => false,
        }
    }
}
