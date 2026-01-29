//! Go tree-sitter queries

use tree_sitter::Language as TsLanguage;

use super::{LanguageQueries, compile_query};

/// Import extraction query for Go
const IMPORTS_QUERY: &str = r#"
; Single import: import "fmt"
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @import))

; Import with alias: import f "fmt"
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @import))

; Import block: import ( "fmt" "os" )
(import_declaration
  (import_spec_list
    (import_spec
      path: (interpreted_string_literal) @import)))
"#;

/// Call extraction query for Go
const CALLS_QUERY: &str = r#"
; Direct function calls: foo()
(call_expression
  function: (identifier) @call)

; Package function calls: fmt.Println()
(call_expression
  function: (selector_expression
    field: (field_identifier) @call))

; Method calls on variables: obj.Method()
(call_expression
  function: (selector_expression
    field: (field_identifier) @call))

; Chained calls: obj.Foo().Bar()
(call_expression
  function: (selector_expression
    operand: (call_expression)
    field: (field_identifier) @call))
"#;

/// Definition extraction query for Go
const DEFINITIONS_QUERY: &str = r#"
; Function declarations
(function_declaration
  name: (identifier) @name) @definition.function

; Method declarations - capture receiver type as parent
; e.g., func (r *Receiver) Method() - parent is "Receiver"
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: (pointer_type
        (type_identifier) @parent)))
  name: (field_identifier) @name) @definition.method

; Method declarations with value receiver
; e.g., func (r Receiver) Method() - parent is "Receiver"
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: (type_identifier) @parent))
  name: (field_identifier) @name) @definition.method

; Type declarations (struct)
(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (struct_type))) @definition.struct

; Type declarations (interface)
(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (interface_type))) @definition.interface

; Type alias
(type_declaration
  (type_spec
    name: (type_identifier) @name)) @definition.type

; Const declarations
(const_declaration
  (const_spec
    name: (identifier) @name)) @definition.const
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
  use crate::{context::files::code::parser::TreeSitterParser, domain::code::Language};

  #[test]
  fn test_go_imports() {
    let content = r#"
package main

import "fmt"
import "os"

import (
    "encoding/json"
    "net/http"
    myalias "github.com/example/pkg"
)
"#;
    let mut parser = TreeSitterParser::new();
    let imports = parser.extract_imports(content, Language::Go);

    assert!(imports.contains(&"fmt".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"os".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"encoding/json".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"net/http".to_string()), "imports: {:?}", imports);
    assert!(
      imports.contains(&"github.com/example/pkg".to_string()),
      "imports: {:?}",
      imports
    );
  }

  #[test]
  fn test_go_calls() {
    let content = r#"
package main

func example() {
    result := helper()
    fmt.Println("hello")
    data := json.Marshal(obj)
    client.Do(req).Body.Close()
}
"#;
    let mut parser = TreeSitterParser::new();
    let calls = parser.extract_calls(content, Language::Go);

    assert!(calls.contains(&"helper".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"Println".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"Marshal".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"Do".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"Close".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_go_definitions() {
    let content = r#"
package main

func myFunction() {}

func (r *Receiver) myMethod() {}

type MyStruct struct {
    Field string
}

type MyInterface interface {
    Method() error
}

const MyConst = 42
"#;
    let mut parser = TreeSitterParser::new();
    let defs = parser.extract_definitions(content, Language::Go);

    let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"myFunction"), "defs: {:?}", names);
    assert!(names.contains(&"myMethod"), "defs: {:?}", names);
    assert!(names.contains(&"MyStruct"), "defs: {:?}", names);
    assert!(names.contains(&"MyInterface"), "defs: {:?}", names);
    assert!(names.contains(&"MyConst"), "defs: {:?}", names);
  }

  #[test]
  fn test_go_method_parent_detection() {
    let content = r#"
package main

type UserRepo struct {
    db *Database
}

func (r *UserRepo) Save(user User) error {
    return r.db.Insert(user)
}

func (r UserRepo) Load(id int) (User, error) {
    return r.db.Get(id)
}

func StandaloneHelper() {
    fmt.Println("helper")
}
"#;
    let mut parser = TreeSitterParser::new();
    let defs = parser.extract_definitions(content, Language::Go);

    // Find methods and verify their parent is 'UserRepo' (from receiver)
    let save_method = defs.iter().find(|d| d.name == "Save");
    assert!(save_method.is_some(), "should find Save method, defs: {:?}", defs);
    assert_eq!(
      save_method.unwrap().parent.as_deref(),
      Some("UserRepo"),
      "Save method should have UserRepo as parent (pointer receiver)"
    );

    let load_method = defs.iter().find(|d| d.name == "Load");
    assert!(load_method.is_some(), "should find Load method");
    assert_eq!(
      load_method.unwrap().parent.as_deref(),
      Some("UserRepo"),
      "Load method should have UserRepo as parent (value receiver)"
    );

    // Verify standalone function has no parent
    let standalone = defs.iter().find(|d| d.name == "StandaloneHelper");
    assert!(standalone.is_some(), "should find StandaloneHelper");
    assert_eq!(
      standalone.unwrap().parent,
      None,
      "standalone function should have no parent"
    );
  }
}
