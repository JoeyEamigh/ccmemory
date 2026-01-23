//! Rust tree-sitter queries

use tree_sitter::Language as TsLanguage;

use super::compile_query;
use crate::parser::LanguageQueries;

/// Import extraction query for Rust
const IMPORTS_QUERY: &str = r#"
; Simple use statements: use foo;
(use_declaration
  argument: (identifier) @import)

; Scoped use: use foo::bar;
(use_declaration
  argument: (scoped_identifier) @import)

; Use list: use foo::{bar, baz}; - capture each item in the list
(use_declaration
  argument: (use_list
    (identifier) @import))
(use_declaration
  argument: (use_list
    (scoped_identifier) @import))

; Scoped use list: use foo::bar::{baz, qux}; - capture items inside the list
(scoped_use_list
  list: (use_list
    (identifier) @import))
(scoped_use_list
  list: (use_list
    (scoped_identifier) @import))

; Also capture aliased items in use lists: use foo::{bar as b, baz};
(scoped_use_list
  list: (use_list
    (use_as_clause
      path: (identifier) @import)))

; Use wildcard: use foo::*;
(use_declaration
  argument: (use_wildcard) @import)

; Aliased use: use foo as bar;
(use_declaration
  argument: (use_as_clause
    path: (identifier) @import))
(use_declaration
  argument: (use_as_clause
    path: (scoped_identifier) @import))
"#;

/// Call extraction query for Rust
const CALLS_QUERY: &str = r#"
; Direct function calls: foo()
(call_expression
  function: (identifier) @call)

; Method calls: obj.method()
(call_expression
  function: (field_expression
    field: (field_identifier) @call))

; Scoped calls: Module::function()
(call_expression
  function: (scoped_identifier
    name: (identifier) @call))

; Generic/turbofish function calls: parse::<Type>(data)
(call_expression
  function: (generic_function
    function: (identifier) @call))

; Generic scoped calls: Vec::<u8>::with_capacity(100)
(call_expression
  function: (generic_function
    function: (scoped_identifier
      name: (identifier) @call)))

; Macro invocations: println!()
(macro_invocation
  macro: (identifier) @call)

; Scoped macro invocations: std::println!()
(macro_invocation
  macro: (scoped_identifier
    name: (identifier) @call))
"#;

/// Definition extraction query for Rust
const DEFINITIONS_QUERY: &str = r#"
; Functions
(function_item
  name: (identifier) @name) @definition.function

; Methods (inside impl blocks)
(impl_item
  body: (declaration_list
    (function_item
      name: (identifier) @name) @definition.method))

; Structs
(struct_item
  name: (type_identifier) @name) @definition.struct

; Enums
(enum_item
  name: (type_identifier) @name) @definition.enum

; Traits
(trait_item
  name: (type_identifier) @name) @definition.trait

; Type aliases
(type_item
  name: (type_identifier) @name) @definition.type

; Modules
(mod_item
  name: (identifier) @name) @definition.module

; Constants
(const_item
  name: (identifier) @name) @definition.const
"#;

pub fn queries(grammar: &TsLanguage) -> LanguageQueries {
  LanguageQueries {
    imports: compile_query(grammar, IMPORTS_QUERY),
    calls: compile_query(grammar, CALLS_QUERY),
    definitions: compile_query(grammar, DEFINITIONS_QUERY),
  }
}

#[cfg(test)]
mod tests {

  use crate::TreeSitterParser;
  use engram_core::Language;

  #[test]
  fn test_rust_imports() {
    let content = r#"
use std::collections::HashMap;
use crate::db::{ProjectDb, Memory};
use super::utils;
use self::helpers::*;
use serde::{Deserialize, Serialize};
"#;
    let mut parser = TreeSitterParser::new();
    let imports = parser.extract_imports(content, Language::Rust);

    // Simple scoped use
    assert!(
      imports.contains(&"std::collections::HashMap".to_string()),
      "imports: {:?}",
      imports
    );
    // Individual items from use lists
    assert!(imports.contains(&"ProjectDb".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"Memory".to_string()), "imports: {:?}", imports);
    // Simple identifier use
    assert!(imports.contains(&"super::utils".to_string()), "imports: {:?}", imports);
    // Wildcard
    assert!(
      imports.contains(&"self::helpers::*".to_string()),
      "imports: {:?}",
      imports
    );
    // Items from serde use list
    assert!(imports.contains(&"Deserialize".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"Serialize".to_string()), "imports: {:?}", imports);
  }

  #[test]
  fn test_rust_calls() {
    let content = r#"
fn example() {
    let x = helper_fn();
    self.method_call();
    Module::associated_fn();
    obj.chain().calls();
    println!("macro");
    vec![];
}
"#;
    let mut parser = TreeSitterParser::new();
    let calls = parser.extract_calls(content, Language::Rust);

    assert!(calls.contains(&"helper_fn".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"method_call".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"associated_fn".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"chain".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"calls".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"println".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"vec".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_rust_definitions() {
    let content = r#"
pub fn my_function() {}

struct MyStruct {
    field: i32,
}

impl MyStruct {
    fn method(&self) {}
}

enum MyEnum {
    A,
    B,
}

trait MyTrait {
    fn trait_method(&self);
}

mod my_module {}
"#;
    let mut parser = TreeSitterParser::new();
    let defs = parser.extract_definitions(content, Language::Rust);

    let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"my_function"), "defs: {:?}", names);
    assert!(names.contains(&"MyStruct"), "defs: {:?}", names);
    assert!(names.contains(&"method"), "defs: {:?}", names);
    assert!(names.contains(&"MyEnum"), "defs: {:?}", names);
    assert!(names.contains(&"MyTrait"), "defs: {:?}", names);
    assert!(names.contains(&"my_module"), "defs: {:?}", names);
  }
}
