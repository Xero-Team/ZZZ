; Semantic symbol definitions for tags/go-to-symbol features.
(function_statement
  (function_name) @name) @definition.function

(class_statement
  (simple_name) @name) @definition.class

(class_method_definition
  (simple_name) @name) @definition.method

(class_property_definition
  (variable) @name) @definition.property

(enum_statement
  (simple_name) @name) @definition.enum

(enum_member
  (simple_name) @name) @definition.constant

; Command and method invocations.
(command
  command_name: (command_name) @name
  (#match? @name "^[A-Za-z_][A-Za-z0-9_.-]*$|^[%?]$")) @reference.call

(command
  command_name: (command_name_expr
    (path_command_name) @name)
  (#match? @name "[:\\/]+")) @reference.call

(invokation_expression
  (member_name) @name) @reference.call

; Type references.
(type_spec
  (type_name) @name) @reference.class

(generic_type_name
  (type_name) @name) @reference.class

(array_type_name
  (type_name) @name) @reference.class
