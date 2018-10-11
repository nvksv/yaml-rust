use std::rc::Rc;
use std::cell::RefCell;

pub trait YamlSettings: Clone {
    fn new() -> Self;
    fn new_safe() -> Self;

    fn is_aliases_allowed(&self) -> bool;
    fn is_multi_doc_allowed(&self) -> bool;
}

#[derive(Clone)]
pub struct YamlStandardSettings {
    v: Rc<RefCell<YamlStandardSettingsData>>,
}

pub struct YamlStandardSettingsData {
    allow_aliases: bool,
    allow_multi_doc: bool,
}

impl YamlStandardSettings {
    
    pub fn new() -> Self {
        let data = YamlStandardSettingsData {
            allow_aliases: true,
            allow_multi_doc: true,
        };
        Self {
            v: Rc::new(RefCell::new(data)),
        }
    }

    pub fn new_safe() -> Self {
        let data = YamlStandardSettingsData {
            allow_aliases: false,
            allow_multi_doc: false,
        };
        Self {
            v: Rc::new(RefCell::new(data)),
        }
    }

    pub fn allow_aliases(self, value: bool) -> Self {
        self.v.borrow_mut().allow_aliases = value;
        self
    }

    pub fn allow_multi_doc(self, value: bool) -> Self {
        self.v.borrow_mut().allow_multi_doc = value;
        self
    }
}

impl YamlSettings for YamlStandardSettings {

    fn new() -> Self {
        YamlStandardSettings::new()
    }

    fn new_safe() -> Self {
        YamlStandardSettings::new_safe()
    }

    fn is_aliases_allowed(&self) -> bool {
        self.v.borrow().allow_aliases
    }

    fn is_multi_doc_allowed(&self) -> bool {
        self.v.borrow().allow_multi_doc
    }
}
