; PowerShell introduces lexical scope at script blocks.
(script_block) @local.scope

; Parameter and loop bindings create new locals within the enclosing script block.
(script_parameter
  (variable) @local.definition)

(class_method_parameter
  (variable) @local.definition)

(foreach_statement
  (variable) @local.definition)

; User-defined locals are commonly introduced by assigning to a bare variable.
(assignment_expression
  (left_assignment_expression
    (logical_expression
      (bitwise_expression
        (comparison_expression
          (additive_expression
            (multiplicative_expression
              (format_expression
                (range_expression
                  (array_literal_expression
                    (unary_expression
                      (variable) @local.definition))))))))))
  (assignement_operator) @operator
  (#eq? @operator "="))

; References exclude automatic/preference variables and explicitly scoped/provider-qualified names.
((variable) @local.reference
  (#not-match?
    @local.reference
    "^\\$(?i:(\\$|\\^|\\?|_|args|consolefilename|error|event|eventargs|eventsubscriber|executioncontext|false|foreach|home|host|input|lastexitcode|matches|myinvocation|nestedpromptlevel|null|pid|profile|psboundparameters|pscmdlet|pscommandpath|psculture|psdebugcontext|pshome|psitem|psscriptroot|pssenderinfo|psuiculture|psversiontable|pwd|sender|shellid|sourceargs|sourceeventargs|stacktrace|switch|this|true|confirmpreference|debugpreference|erroractionpreference|errorview|formatenumerationlimit|informationpreference|logcommandhealthevent|logcommandlifecycleevent|logenginehealthevent|logenginelifecycleevent|logproviderhealthevent|logproviderlifecycleevent|maximumaliascount|maximumdrivecount|maximumerrorcount|maximumfunctioncount|maximumhistorycount|maximumvariablecount|ofs|outputencoding|progresspreference|psdefaultparametervalues|pstranscriptionpreference|verbosepreference|warningpreference|whatifpreference))$")
  (#not-match? @local.reference "^\\$(?i:(global|local|private|script|using|workflow|alias|env|function|variable):)"))

((braced_variable) @local.reference
  (#not-match? @local.reference "^\\$\\{(?i:(global|local|private|script|using|workflow|alias|env|function|variable):)"))
