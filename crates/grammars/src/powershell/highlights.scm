; Keywords exposed as anonymous tokens by the current grammar.
[
  "begin"
  "break"
  "catch"
  "clean"
  "continue"
  "data"
  "do"
  "dynamicparam"
  "else"
  "elseif"
  "end"
  "exit"
  "filter"
  "finally"
  "for"
  "foreach"
  "function"
  "if"
  "in"
  "inlinescript"
  "parallel"
  "param"
  "process"
  "return"
  "sequence"
  "switch"
  "throw"
  "trap"
  "try"
  "until"
  "using"
  "while"
  "workflow"
] @keyword

; Operators that are exposed as plain tokens by the grammar.
[
  "!"
  "&"
  "+"
  "-"
  "--"
  "."
  "/"
  "::"
  "\\"
  "|"
  "%"
  "*"
  "++"
  ".."
] @operator

(assignement_operator) @operator
(comparison_operator) @operator
(command_invokation_operator) @operator
(file_redirection_operator) @operator
(merging_redirection_operator) @operator

[
  ","
  ";"
] @punctuation.delimiter

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
  "@("
  "@{"
  "$("
] @punctuation.bracket

[
  (string_literal)
  (expandable_string_literal)
  (expandable_here_string_literal)
  (verbatim_string_characters)
  (verbatim_here_string_characters)
] @string

[
  (integer_literal)
  (decimal_integer_literal)
  (hexadecimal_integer_literal)
  (real_literal)
] @number

((variable) @constant.builtin
  (#match? @constant.builtin "^\\$(?i:(true|false|null))$"))

((braced_variable) @constant.builtin
  (#match? @constant.builtin "^\\$\\{(?i:(true|false|null))\}$"))

; Automatic and preference variables from the spec, plus current pwsh runtime variables.
((variable) @variable.builtin
  (#match?
    @variable.builtin
    "^\\$(?i:(\\$|\\^|\\?|_|args|consolefilename|error|event|eventargs|eventsubscriber|executioncontext|foreach|home|host|input|lastexitcode|matches|myinvocation|nestedpromptlevel|pid|profile|psboundparameters|pscmdlet|pscommandpath|psculture|psdebugcontext|pshome|psitem|psscriptroot|pssenderinfo|psuiculture|psversiontable|pwd|sender|shellid|sourceargs|sourceeventargs|stacktrace|switch|this|confirmpreference|debugpreference|erroractionpreference|errorview|formatenumerationlimit|informationpreference|logcommandhealthevent|logcommandlifecycleevent|logenginehealthevent|logenginelifecycleevent|logproviderhealthevent|logproviderlifecycleevent|maximumaliascount|maximumdrivecount|maximumerrorcount|maximumfunctioncount|maximumhistorycount|maximumvariablecount|ofs|outputencoding|progresspreference|psdefaultparametervalues|pstranscriptionpreference|verbosepreference|warningpreference|whatifpreference))$"))

((braced_variable) @variable.builtin
  (#match?
    @variable.builtin
    "^\\$\\{(?i:(\\$|\\^|\\?|_|args|consolefilename|error|event|eventargs|eventsubscriber|executioncontext|foreach|home|host|input|lastexitcode|matches|myinvocation|nestedpromptlevel|pid|profile|psboundparameters|pscmdlet|pscommandpath|psculture|psdebugcontext|pshome|psitem|psscriptroot|pssenderinfo|psuiculture|psversiontable|pwd|sender|shellid|sourceargs|sourceeventargs|stacktrace|switch|this|confirmpreference|debugpreference|erroractionpreference|errorview|formatenumerationlimit|informationpreference|logcommandhealthevent|logcommandlifecycleevent|logenginehealthevent|logenginelifecycleevent|logproviderhealthevent|logproviderlifecycleevent|maximumaliascount|maximumdrivecount|maximumerrorcount|maximumfunctioncount|maximumhistorycount|maximumvariablecount|ofs|outputencoding|progresspreference|psdefaultparametervalues|pstranscriptionpreference|verbosepreference|warningpreference|whatifpreference))\}$"))

(command
  command_name: (command_name) @function)

(path_command_name) @function

(function_statement
  (function_name) @function)

(invokation_expression
  (member_name) @function)

(member_access
  (member_name) @property)

(command_parameter) @parameter

(block_name) @keyword

(class_keyword) @keyword

(class_attribute) @keyword

(enum_keyword) @keyword

(foreach_parameter) @keyword

(switch_parameter) @keyword

(script_parameter
  (variable) @parameter)

(class_method_parameter
  (variable) @parameter)

(type_spec) @type

(format_operator) @operator

(stop_arguments
  "--" @operator)

(stop_parsing) @string.special

((type_spec) @type.builtin
  (#match? @type.builtin "^(?i:bool|byte|char|datetime|decimal|double|float|guid|hashtable|int|int16|int32|int64|long|object|ordered|pscustomobject|regex|sbyte|scriptblock|single|string|switch|timespan|type|uint|uint16|uint32|uint64|uri|version|void|xml)$"))

(attribute
  (attribute_name) @attribute)

(label) @label

(variable) @variable

(braced_variable) @variable

(comment) @comment

; Highlight TODO/NOTE/WARNING markers inside comments.
(
  (comment) @comment.todo
  (#match? @comment.todo "TODO:")
)
(
  (comment) @comment.note
  (#match? @comment.note "NOTE:")
)
(
  (comment) @comment.warning
  (#match? @comment.warning "WARNING:|WARN:|ATTENTION:")
)
