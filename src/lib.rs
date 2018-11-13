// Copyright 2015, Yuheng Chen. See the LICENSE file at the top-level
// directory of this distribution.

//! YAML 1.2 implementation in pure Rust.
//!
//! # Usage
//!
//! This crate is [on github](https://github.com/chyh1990/yaml-rust) and can be
//! used by adding `yaml-rust` to the dependencies in your project's `Cargo.toml`.
//!
//! ```toml
//! [dependencies.yaml-rust]
//! git = "https://github.com/chyh1990/yaml-rust.git"
//! ```
//!
//! And this in your crate root:
//!
//! ```rust
//! extern crate yaml_rust;
//! ```
//!
//! Parse a string into `Vec<Yaml>` and then serialize it as a YAML string.
//!
//! # Examples
//!
//! ```
//! use yaml_rust::{yaml_load_from_str, yaml_dump};
//!
//! let docs = yaml_load_from_str("[1, 2, 3]").unwrap();
//! let doc = &docs[0]; // select the first document
//! assert_eq!(doc[0].as_i64().unwrap(), 1); // access elements by index
//!
//! let mut out_str = String::new();
//! yaml_dump(&mut out_str, doc).unwrap(); // dump the YAML object to a String
//!
//! ```

#![doc(html_root_url = "https://docs.rs/yaml-rust/0.4.2")]
#![cfg_attr(feature = "cargo-clippy", allow(renamed_and_removed_lints))]
#![cfg_attr(feature = "cargo-clippy", warn(cyclomatic_complexity))]
#![cfg_attr(
    feature = "cargo-clippy",
    allow(match_same_arms, should_implement_trait)
)]

extern crate linked_hash_map;

pub mod emitter;
pub mod parser;
pub mod scanner;
pub mod yaml;
pub mod loader;
pub mod settings;
pub mod builder;

// reexport key APIs
pub use emitter::{EmitError, YamlEmitter, yaml_dump, yaml_dump_compact};
pub use parser::Event;
pub use scanner::ScanError;
pub use yaml::Yaml;
pub use loader::{YamlLoader, yaml_load_from_str, yaml_load_doc_from_str, yaml_load_from_str_safe, yaml_load_doc_from_str_safe};
pub use settings::{YamlSettings, YamlStandardSettings};
pub use builder::{YamlBuilder, YamlNodeKind, YamlStandardBuilder};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api() {
        let s = "
# from yaml-cpp example
- name: Ogre
  position: [0, 5, 0]
  powers:
    - name: Club
      damage: 10
    - name: Fist
      damage: 8
- name: Dragon
  position: [1, 0, 10]
  powers:
    - name: Fire Breath
      damage: 25
    - name: Claws
      damage: 15
- name: Wizard
  position: [5, -3, 0]
  powers:
    - name: Acid Rain
      damage: 50
    - name: Staff
      damage: 3
";
        let doc = yaml_load_doc_from_str(s).unwrap();

        assert_eq!(doc[0]["name"].as_str().unwrap(), "Ogre");

        let mut writer = String::new();
        yaml_dump(&mut writer, &doc).unwrap();

        assert!(!writer.is_empty());
    }

    fn try_fail(s: &str) -> Result<Vec<Yaml>, ScanError> {
        let t = yaml_load_from_str(s)?;
        Ok(t)
    }

    #[test]
    fn test_fail() {
        let s = "
# syntax error
scalar
key: [1, 2]]
key1:a2
";
        assert!(yaml_load_from_str(s).is_err());
        assert!(try_fail(s).is_err());
    }

}
