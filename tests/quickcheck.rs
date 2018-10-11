extern crate yaml_rust;
#[macro_use]
extern crate quickcheck;

use quickcheck::TestResult;
use std::error::Error;
use yaml_rust::{Yaml, yaml_dump, yaml_load_from_str};

quickcheck! {
    fn test_check_weird_keys(xs: Vec<String>) -> TestResult {
        let mut out_str = String::new();
        let input = Yaml::Array(xs.into_iter().map(Yaml::String).collect());
        yaml_dump(&mut out_str, &input).unwrap();

        match yaml_load_from_str(&out_str) {
            Ok(output) => TestResult::from_bool(output.len() == 1 && input == output[0]),
            Err(err) => TestResult::error(err.description()),
        }
    }
}
