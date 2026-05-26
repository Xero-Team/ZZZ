; Structural outline items for the outline panel and file summaries.
; Keep this query focused on declarations that should appear in the hierarchy,
; while tags.scm owns semantic definitions/references for symbol navigation.

; Functions, filters, and workflows.
(function_statement
  (function_name) @name) @item

; Classes
(class_statement
  (simple_name) @name) @item

; Class methods
(class_method_definition
  (simple_name) @name) @item

; Class properties
(class_property_definition
  (variable) @name) @item

; Enums
(enum_statement
  (simple_name) @name) @item

; Enum members
(enum_member
  (simple_name) @name) @item

; Named blocks (begin, process, end, dynamicparam)
(named_block
  (block_name) @name) @item
