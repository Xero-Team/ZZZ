// @ts-nocheck

function ci(keyword) {
  return new RegExp(keyword.split("").map((character) => {
    if (/[^A-Za-z0-9]/.test(character)) {
      return `\\${character}`;
    }

    return `[${character.toLowerCase()}${character.toUpperCase()}]`;
  }).join(""));
}

const setArithmeticPrecedence = {
  comma: -1,
  assignment: 0,
  bitwiseOr: 1,
  bitwiseXor: 2,
  bitwiseAnd: 3,
  shift: 4,
  additive: 5,
  multiplicative: 6,
  unary: 7,
};

const comparisonOperators = ["EQU", "NEQ", "LSS", "LEQ", "GTR", "GEQ"];

module.exports = grammar({
  name: "cmd",

  extras: () => [/[ \t\f]/],

  word: $ => $.word,

  conflicts: $ => [
    [$.argument, $.for_set_item],
    [$.start_statement, $.command_argument],
    [$.findstr_pattern, $.findstr_file_argument],
  ],

  rules: {
    source_file: $ => seq(
      repeat(seq(optional($.line), $._newline)),
      optional($.line),
    ),

    line: $ => choice($.label, $.comment, $.statement),

    statement: $ => $.command_chain,

    command_chain: $ => prec.left(seq(
      $.command,
      repeat(seq(field("operator", $.command_operator), $.command)),
    )),

    command: $ => prec.right(seq(
      optional(field("prefix", $.command_prefix)),
      repeat($.redirection),
      choice(
        $.attrib_statement,
        $.assoc_statement,
        $.ftype_statement,
        $.cd_statement,
        $.pushd_statement,
        $.popd_statement,
        $.mkdir_statement,
        $.rmdir_statement,
        $.if_statement,
        $.for_statement,
        $.path_statement,
        $.prompt_statement,
        $.setlocal_statement,
        $.endlocal_statement,
        $.help_statement,
        $.type_statement,
        $.verify_statement,
        $.vol_statement,
        $.title_statement,
        $.color_statement,
        $.date_statement,
        $.time_statement,
        $.set_statement,
        $.break_statement,
        $.call_label_statement,
        $.call_batch_statement,
        $.goto_eof_statement,
        $.goto_label_statement,
        $.shift_statement,
        $.exit_statement,
        $.start_statement,
        $.copy_statement,
        $.move_statement,
        $.xcopy_statement,
        $.del_statement,
        $.rename_statement,
        $.more_file_list_statement,
        $.more_stream_statement,
        $.find_statement,
        $.mklink_statement,
        $.dir_statement,
        $.findstr_statement,
        $.tree_statement,
        $.chcp_statement,
        $.subst_statement,
        $.sort_statement,
        $.fc_statement,
        $.comp_statement,
        $.mode_statement,
        $.doskey_statement,
        $.tasklist_statement,
        $.taskkill_statement,
        $.systeminfo_statement,
        $.shutdown_statement,
        $.driverquery_statement,
        $.openfiles_statement,
        $.schtasks_statement,
        $.sc_statement,
        $.gpresult_statement,
        $.bcdedit_statement,
        $.compact_statement,
        $.replace_statement,
        $.convert_statement,
        $.chkdsk_statement,
        $.chkntfs_statement,
        $.print_statement,
        $.icacls_statement,
        $.cacls_statement,
        $.recover_statement,
        $.format_statement,
        $.fsutil_statement,
        $.label_statement,
        $.cmd_statement,
        $.robocopy_statement,
        $.echo_statement,
        $.pause_statement,
        $.cls_statement,
        $.ver_statement,
        $.simple_command,
        $.command_group,
      ),
      repeat($.redirection),
    )),

    command_group: $ => seq(
      "(",
      repeat(seq(optional($.line), $._newline)),
      optional($.line),
      ")",
    ),

    if_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("if")), $.keyword)),
      optional(field("negation", alias(token(ci("not")), $.keyword))),
      optional(field("flag", $.if_flag)),
      field("condition", choice(
        $.if_errorlevel_condition,
        $.if_exist_condition,
        $.if_defined_condition,
        $.if_cmdextversion_condition,
        $.if_compare_condition,
        $.if_string_condition,
      )),
      field("consequence", $.command),
      optional(seq(
        field("else_keyword", alias(token(ci("else")), $.keyword)),
        field("alternative", $.command),
      )),
    )),

    if_flag: _ => token(ci("/i")),

    if_errorlevel_condition: $ => seq(
      alias(token(ci("errorlevel")), $.keyword),
      field("value", $.number),
    ),

    if_exist_condition: $ => seq(
      alias(token(ci("exist")), $.keyword),
      field("path", $.argument),
    ),

    if_defined_condition: $ => seq(
      alias(token(ci("defined")), $.keyword),
      field("name", $.word),
    ),

    if_cmdextversion_condition: $ => seq(
      alias(token(ci("cmdextversion")), $.keyword),
      field("value", $.number),
    ),

    if_compare_condition: $ => seq(
      field("left", $._if_operand),
      field("operator", $.comparison_operator),
      field("right", $._if_operand),
    ),

    if_string_condition: $ => seq(
      field("left", $._if_operand),
      field("operator", alias("==", $.comparison_operator)),
      field("right", $._if_operand),
    ),

    for_statement: $ => choice(
      $.for_f_usebackq_statement,
      $.for_f_default_statement,
      $.for_range_statement,
      $.for_recursive_statement,
      $.for_directory_statement,
      $.for_basic_statement,
    ),

    for_basic_statement: $ => seq(
      field("keyword", alias(token(ci("for")), $.keyword)),
      field("variable", $.for_variable),
      field("in_keyword", alias(token(ci("in")), $.keyword)),
      field("set", $.for_enumeration_set),
      field("do_keyword", alias(token(ci("do")), $.keyword)),
      field("body", $.command),
    ),

    for_directory_statement: $ => seq(
      field("keyword", alias(token(ci("for")), $.keyword)),
      field("mode", $.for_directory_mode),
      field("variable", $.for_variable),
      field("in_keyword", alias(token(ci("in")), $.keyword)),
      field("set", $.for_enumeration_set),
      field("do_keyword", alias(token(ci("do")), $.keyword)),
      field("body", $.command),
    ),

    for_recursive_statement: $ => seq(
      field("keyword", alias(token(ci("for")), $.keyword)),
      field("mode", $.for_recursive_mode),
      optional(field("root", $.for_root)),
      field("variable", $.for_variable),
      field("in_keyword", alias(token(ci("in")), $.keyword)),
      field("set", $.for_enumeration_set),
      field("do_keyword", alias(token(ci("do")), $.keyword)),
      field("body", $.command),
    ),

    for_range_statement: $ => seq(
      field("keyword", alias(token(ci("for")), $.keyword)),
      field("mode", $.for_range_mode),
      field("variable", $.for_variable),
      field("in_keyword", alias(token(ci("in")), $.keyword)),
      field("set", $.for_range_set),
      field("do_keyword", alias(token(ci("do")), $.keyword)),
      field("body", $.command),
    ),

    for_f_default_statement: $ => seq(
      field("keyword", alias(token(ci("for")), $.keyword)),
      field("mode", $.for_file_parsing_mode),
      optional(field("options", $.for_f_default_options)),
      field("variable", $.for_variable),
      field("in_keyword", alias(token(ci("in")), $.keyword)),
      field("set", $.for_f_default_set),
      field("do_keyword", alias(token(ci("do")), $.keyword)),
      field("body", $.command),
    ),

    for_f_usebackq_statement: $ => prec(1, seq(
      field("keyword", alias(token(ci("for")), $.keyword)),
      field("mode", $.for_file_parsing_mode),
      field("options", $.for_f_usebackq_options),
      field("variable", $.for_variable),
      field("in_keyword", alias(token(ci("in")), $.keyword)),
      field("set", $.for_f_usebackq_set),
      field("do_keyword", alias(token(ci("do")), $.keyword)),
      field("body", $.command),
    )),

    for_directory_mode: _ => token(ci("/d")),
    for_recursive_mode: _ => token(ci("/r")),
    for_range_mode: _ => token(ci("/l")),
    for_file_parsing_mode: _ => token(ci("/f")),

    for_root: $ => choice(
      $.path,
      $.for_current_directory,
    ),

    for_current_directory: _ => ".",

    for_enumeration_set: $ => seq(
      "(",
      repeat(choice($.for_set_item, $.command_operator, ",", ";", "=")),
      ")",
    ),

    for_set_item: $ => choice(
      $.argument,
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.for_loop_range,
    ),

    for_range_set: $ => seq(
      "(",
      $.for_loop_range,
      ")",
    ),

    for_f_default_options: _ => token(/"[^"\r\n]*"/),
    for_f_usebackq_options: _ => token(prec(1, /"[^"\r\n]*[Uu][Ss][Ee][Bb][Aa][Cc][Kk][Qq][^"\r\n]*"/)),

    for_f_default_set: $ => seq(
      "(",
      field("source", choice(
        $.for_f_file_set,
        $.for_f_default_literal_source,
        $.for_f_default_command_source,
      )),
      ")",
    ),

    for_f_usebackq_set: $ => seq(
      "(",
      field("source", choice(
        $.for_f_file_set,
        $.for_f_usebackq_file_source,
        $.for_f_usebackq_literal_source,
        $.for_f_usebackq_command_source,
      )),
      ")",
    ),

    for_f_file_set: _ => token(/[^()"'`\r\n][^)\r\n]*/),
    for_f_default_literal_source: $ => $.quoted_string,
    for_f_default_command_source: $ => $.single_quoted_string,
    for_f_usebackq_file_source: $ => $.quoted_string,
    for_f_usebackq_literal_source: $ => $.single_quoted_string,
    for_f_usebackq_command_source: $ => $.backquoted_string,

    for_loop_range: $ => prec(1, seq(
      field("start", $.number),
      ",",
      field("step", $.number),
      ",",
      field("end", $.number),
    )),

    set_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("set")), $.keyword)),
      optional(choice(
        $.set_arithmetic_clause,
        $.set_prompt_clause,
        $.set_assignment_clause,
        $.set_query,
      )),
    )),

    set_arithmetic_clause: $ => seq(
      field("flag", $.set_arithmetic_flag),
      field("expression", choice(
        $.set_arithmetic_expression,
        $.set_arithmetic_quoted_expression,
      )),
    ),

    set_prompt_clause: $ => seq(
      field("flag", $.set_prompt_flag),
      field("assignment", $.set_prompt_assignment),
    ),

    set_assignment_clause: $ => $.set_assignment,

    set_query: $ => field("name", $.set_variable_name),

    set_assignment: $ => prec.right(seq(
      field("name", $.set_variable_name),
      "=",
      optional(field("value", $.set_value)),
    )),

    set_prompt_assignment: $ => prec.right(seq(
      field("name", $.set_variable_name),
      "=",
      optional(field("prompt", $.set_value)),
    )),

    set_arithmetic_quoted_expression: $ => seq(
      '"',
      field("expression", $.set_arithmetic_expression),
      token.immediate('"'),
    ),

    set_arithmetic_expression: $ => choice(
      $.set_arithmetic_comma_expression,
      $.set_arithmetic_assignment_expression,
    ),

    set_arithmetic_comma_expression: $ => prec.left(setArithmeticPrecedence.comma, seq(
      field("left", $.set_arithmetic_assignment_expression),
      field("operator", $.set_arithmetic_comma_operator),
      field("right", $.set_arithmetic_expression),
    )),

    set_arithmetic_assignment_expression: $ => choice(
      prec.right(setArithmeticPrecedence.assignment, seq(
        field("left", $.set_arithmetic_lvalue),
        field("operator", $.set_arithmetic_assignment_operator),
        field("right", $.set_arithmetic_assignment_expression),
      )),
      $.set_arithmetic_bitwise_or_expression,
    ),

    set_arithmetic_bitwise_or_expression: $ => choice(
      prec.left(setArithmeticPrecedence.bitwiseOr, seq(
        field("left", $.set_arithmetic_bitwise_or_expression),
        field("operator", $.set_arithmetic_bitwise_or_operator),
        field("right", $.set_arithmetic_bitwise_xor_expression),
      )),
      $.set_arithmetic_bitwise_xor_expression,
    ),

    set_arithmetic_bitwise_xor_expression: $ => choice(
      prec.left(setArithmeticPrecedence.bitwiseXor, seq(
        field("left", $.set_arithmetic_bitwise_xor_expression),
        field("operator", $.set_arithmetic_bitwise_xor_operator),
        field("right", $.set_arithmetic_bitwise_and_expression),
      )),
      $.set_arithmetic_bitwise_and_expression,
    ),

    set_arithmetic_bitwise_and_expression: $ => choice(
      prec.left(setArithmeticPrecedence.bitwiseAnd, seq(
        field("left", $.set_arithmetic_bitwise_and_expression),
        field("operator", $.set_arithmetic_bitwise_and_operator),
        field("right", $.set_arithmetic_shift_expression),
      )),
      $.set_arithmetic_shift_expression,
    ),

    set_arithmetic_shift_expression: $ => choice(
      prec.left(setArithmeticPrecedence.shift, seq(
        field("left", $.set_arithmetic_shift_expression),
        field("operator", $.set_arithmetic_shift_operator),
        field("right", $.set_arithmetic_additive_expression),
      )),
      $.set_arithmetic_additive_expression,
    ),

    set_arithmetic_additive_expression: $ => choice(
      prec.left(setArithmeticPrecedence.additive, seq(
        field("left", $.set_arithmetic_additive_expression),
        field("operator", $.set_arithmetic_additive_operator),
        field("right", $.set_arithmetic_multiplicative_expression),
      )),
      $.set_arithmetic_multiplicative_expression,
    ),

    set_arithmetic_multiplicative_expression: $ => choice(
      prec.left(setArithmeticPrecedence.multiplicative, seq(
        field("left", $.set_arithmetic_multiplicative_expression),
        field("operator", $.set_arithmetic_multiplicative_operator),
        field("right", $.set_arithmetic_unary_expression),
      )),
      $.set_arithmetic_unary_expression,
    ),

    set_arithmetic_unary_expression: $ => choice(
      prec.right(setArithmeticPrecedence.unary, seq(
        field("operator", $.set_arithmetic_unary_operator),
        field("operand", $.set_arithmetic_unary_expression),
      )),
      $.set_arithmetic_primary_expression,
    ),

    set_arithmetic_primary_expression: $ => choice(
      $.set_arithmetic_parenthesized_expression,
      $.set_arithmetic_number,
      $.set_arithmetic_identifier,
      $.variable_expansion,
      $.delayed_variable,
    ),

    set_arithmetic_parenthesized_expression: $ => seq(
      "(",
      $.set_arithmetic_expression,
      ")",
    ),

    set_arithmetic_lvalue: $ => $.set_arithmetic_identifier,

    set_arithmetic_comma_operator: _ => ",",
    set_arithmetic_assignment_operator: _ => token(choice("<<=", ">>=", "*=", "/=", "%=", "+=", "-=", "&=", "^=", "|=", "=")),
    set_arithmetic_bitwise_or_operator: _ => "|",
    set_arithmetic_bitwise_xor_operator: _ => "^",
    set_arithmetic_bitwise_and_operator: _ => "&",
    set_arithmetic_shift_operator: _ => token(choice("<<", ">>")),
    set_arithmetic_additive_operator: _ => choice("+", "-"),
    set_arithmetic_multiplicative_operator: _ => choice("*", "/", "%"),
    set_arithmetic_unary_operator: _ => choice("!", "~", "-", "+"),

    set_arithmetic_number: _ => token(choice(
      /0[xX][0-9A-Fa-f]+/,
      /0[0-7]+/,
      /[0-9]+/,
    )),
    set_arithmetic_identifier: _ => token(/[A-Za-z_][A-Za-z0-9_]*/),

    set_variable_name: _ => token(/[^ \t\r\n=\/&|<>]+/),
    set_value: $ => prec.right(repeat1(choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.for_variable,
      $.path,
      $.number,
      $.escaped_character,
      $.argument_text,
      "=",
    ))),
    set_arithmetic_flag: _ => token(ci("/a")),
    set_prompt_flag: _ => token(ci("/p")),

    call_label_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("call")), $.keyword)),
      field("target", $.call_label_target),
      repeat(field("argument", $.call_label_argument)),
    )),

    call_batch_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("call")), $.keyword)),
      field("target", $.call_batch_target),
      repeat(field("argument", $.call_batch_argument)),
    )),

    goto_eof_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("goto")), $.keyword)),
      field("target", $.goto_eof_target),
    )),

    goto_label_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("goto")), $.keyword)),
      field("target", $.goto_label_target),
    )),

    shift_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("shift")), $.keyword)),
      optional(field("amount", $.shift_flag)),
    )),

    break_statement: $ => seq(
      field("keyword", alias(token(ci("break")), $.keyword)),
    ),

    pause_statement: $ => seq(
      field("keyword", alias(token(ci("pause")), $.keyword)),
    ),

    cls_statement: $ => seq(
      field("keyword", alias(token(ci("cls")), $.keyword)),
    ),

    ver_statement: $ => seq(
      field("keyword", alias(token(ci("ver")), $.keyword)),
    ),

    shift_flag: _ => token(/\/[0-9]/),

    attrib_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("attrib")), $.keyword)),
      repeat(field("attribute", $.attrib_attribute_change)),
      optional(field("target", $.attrib_target)),
      repeat(field("option", $.attrib_flag)),
    )),

    attrib_attribute_change: _ => token(/[+-][RrAaSsHhOoIiXxVvPpUu]/),
    attrib_target: $ => $.move_path_argument,
    attrib_flag: _ => token(choice(ci("/s"), ci("/d"), ci("/l"))),

    assoc_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("assoc")), $.keyword)),
      optional(choice(
        $.assoc_assignment,
        $.assoc_query,
      )),
    )),

    assoc_query: $ => field("extension", $.assoc_extension),
    assoc_assignment: $ => seq(
      field("extension", $.assoc_extension),
      "=",
      optional(field("file_type", $.assoc_file_type)),
    ),
    assoc_extension: _ => token(/\.[^ \t\r\n=&|<>()"'`%]+/),
    assoc_file_type: _ => token(/[^ \t\r\n=&|<>()"'`%]+/),

    cd_statement: $ => prec.right(seq(
      field("keyword", alias(token(choice(ci("cd"), ci("chdir"))), $.keyword)),
      optional(field("flag", $.cd_flag)),
      optional(choice(
        field("query", $.cd_drive_query),
        field("target", $.cd_target),
      )),
    )),
    cd_flag: _ => token(ci("/d")),
    cd_drive_query: _ => token(/[A-Za-z]:/),
    cd_target: $ => choice(
      $.cd_parent,
      $.cd_path_argument,
    ),
    cd_parent: _ => "..",
    cd_path_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.cd_path_text,
    ),

    pushd_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("pushd")), $.keyword)),
      optional(field("target", choice($.cd_parent, $.cd_path_argument))),
    )),

    popd_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("popd")), $.keyword)),
    )),

    mkdir_statement: $ => prec.right(seq(
      field("keyword", alias(token(choice(ci("md"), ci("mkdir"))), $.keyword)),
      field("target", $.cd_path_argument),
    )),

    rmdir_statement: $ => prec.right(seq(
      field("keyword", alias(token(choice(ci("rd"), ci("rmdir"))), $.keyword)),
      repeat(field("option", $.rmdir_flag)),
      field("target", $.cd_path_argument),
    )),
    rmdir_flag: _ => token(choice(ci("/s"), ci("/q"))),

    ftype_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("ftype")), $.keyword)),
      optional(choice(
        $.ftype_assignment,
        $.ftype_query,
      )),
    )),

    ftype_query: $ => field("file_type", $.assoc_file_type),
    ftype_assignment: $ => prec.right(seq(
      field("file_type", $.assoc_file_type),
      "=",
      optional(field("command", $.ftype_command)),
    )),
    ftype_command: $ => prec.right(seq(
      field("executable", $.ftype_command_atom),
      repeat(field("argument", $.ftype_command_atom)),
    )),
    ftype_command_atom: $ => choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.for_variable,
      $.path,
      $.number,
      $.escaped_character,
      $.ftype_command_text,
    ),

    path_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("path")), $.keyword)),
      optional(choice(
        $.path_clear,
        $.path_assignment,
      )),
    )),

    path_clear: _ => prec(1, ";"),
    path_assignment: $ => seq(
      field("entry", $.path_entry),
      repeat(seq(
        field("separator", $.path_separator),
        optional(field("entry", $.path_entry)),
      )),
    ),
    path_entry: $ => choice(
      $.path_directory_entry,
      $.path_existing_path_reference,
      $.path_variable_reference,
    ),
    path_directory_entry: $ => $.path_value_segment,
    path_existing_path_reference: _ => token(prec(1, /%[Pp][Aa][Tt][Hh]%/)),
    path_variable_reference: $ => choice(
      $.variable_expansion,
      $.delayed_variable,
    ),
    path_separator: _ => ";",

    prompt_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("prompt")), $.keyword)),
      optional(field("value", $.prompt_value)),
    )),

    prompt_value: $ => prec.right(repeat1(choice(
      $.prompt_code,
      $.prompt_text_segment,
      $.variable_expansion,
      $.delayed_variable,
      $.escaped_character,
      $.path,
      $.number,
    ))),
    prompt_code: _ => token(/\$(?:[AaBbCcDdEeFfGgHhLlMmNnPpQqSsTtVv_+$])/),
    prompt_text_segment: _ => token(/[^ \t\r\n$&|<>()"'`%]+/),

    verify_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("verify")), $.keyword)),
      optional(field("mode", $.verify_mode)),
    )),
    verify_mode: _ => token(choice(ci("on"), ci("off"))),

    vol_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("vol")), $.keyword)),
      optional(field("drive", $.vol_drive)),
    )),
    vol_drive: _ => token(/[A-Za-z]:/),

    title_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("title")), $.keyword)),
      optional(field("value", $.title_assignment)),
    )),
    title_assignment: $ => prec.right(repeat1(choice(
      alias($.quoted_string, $.title_quoted_fragment),
      alias($.variable_expansion, $.title_variable_expansion),
      alias($.delayed_variable, $.title_delayed_variable),
      alias($.escaped_character, $.title_escape_fragment),
      $.title_text_fragment,
    ))),

    color_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("color")), $.keyword)),
      optional(field("value", $.color_attribute)),
    )),
    color_attribute: _ => token(/[0-9A-Fa-f]{2}/),

    date_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("date")), $.keyword)),
      optional(choice(
        field("flag", $.date_time_flag),
        field("value", $.date_value),
      )),
    )),
    time_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("time")), $.keyword)),
      optional(choice(
        field("flag", $.date_time_flag),
        field("value", $.time_value),
      )),
    )),
    date_time_flag: _ => token(ci("/t")),
    date_value: $ => choice(
      seq(
        field("first", $.date_time_number),
        field("separator", $.date_separator),
        field("second", $.date_time_number),
        field("separator", $.date_separator),
        field("third", $.date_time_number),
      ),
      seq(
        field("first", $.date_time_number),
        field("separator", $.date_separator),
        field("second", $.date_time_number),
      ),
    ),
    time_value: $ => choice(
      seq(
        field("hour", $.date_time_number),
        ":",
        field("minute", $.date_time_number),
        ":",
        field("second", $.date_time_number),
        ".",
        field("fraction", $.date_time_number),
        optional(field("period", $.time_period)),
      ),
      seq(
        field("hour", $.date_time_number),
        ":",
        field("minute", $.date_time_number),
        ":",
        field("second", $.date_time_number),
        optional(field("period", $.time_period)),
      ),
      seq(
        field("hour", $.date_time_number),
        ":",
        field("minute", $.date_time_number),
        optional(field("period", $.time_period)),
      ),
    ),
    date_time_number: _ => token(/[0-9]{1,4}/),
    date_separator: _ => token(prec(1, /[\/.:-]/)),
    time_period: _ => token(/[AaPp][Mm]/),

    exit_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("exit")), $.keyword)),
      optional(field("flag", $.exit_flag)),
      optional(field("code", $.number)),
    )),

    exit_flag: _ => token(ci("/b")),

    start_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("start")), $.keyword)),
      optional(field("title", $.quoted_string)),
      repeat(field("option", choice(
        $.start_directory_option,
        $.start_node_option,
        $.start_affinity_option,
        $.start_machine_option,
        $.start_priority_option,
        $.start_flag,
      ))),
      optional($.start_command_tail),
    )),

    start_command_tail: $ => choice(
      $.start_cmd_shell_tail,
      $.start_program_tail,
    ),
    start_cmd_shell_tail: $ => prec.right(seq(
      field("command", $.start_cmd_program),
      field("mode", $.start_cmd_switch),
      field("command_line", $.start_shell_command),
    )),
    start_program_tail: $ => prec.right(seq(
      field("command", $.command_argument),
      repeat(field("parameter", $.command_argument)),
    )),
    start_cmd_program: _ => token(choice(ci("cmd"), ci("cmd.exe"))),
    start_cmd_switch: _ => token(choice(ci("/c"), ci("/k"))),
    start_shell_command: $ => prec.right(repeat1(choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.for_variable,
      $.path,
      $.number,
      $.escaped_character,
      $.argument_text,
    ))),

    start_directory_option: $ => seq(
      field("flag", $.start_directory_flag),
      field("path", $.command_argument),
    ),
    start_node_option: $ => seq(
      field("flag", $.start_node_flag),
      field("value", $.number),
    ),
    start_affinity_option: $ => seq(
      field("flag", $.start_affinity_flag),
      field("value", $.start_affinity_mask),
    ),
    start_machine_option: $ => seq(
      field("flag", $.start_machine_flag),
      field("architecture", $.start_machine_architecture),
    ),
    start_priority_option: _ => token(choice(
      ci("/low"),
      ci("/normal"),
      ci("/high"),
      ci("/realtime"),
      ci("/abovenormal"),
      ci("/belownormal"),
    )),
    start_flag: _ => token(choice(
      ci("/i"),
      ci("/min"),
      ci("/max"),
      ci("/separate"),
      ci("/shared"),
      ci("/wait"),
      ci("/b"),
    )),
    start_directory_flag: _ => token(ci("/d")),
    start_node_flag: _ => token(ci("/node")),
    start_affinity_flag: _ => token(ci("/affinity")),
    start_machine_flag: _ => token(ci("/machine")),
    start_affinity_mask: _ => token(/0[xX][0-9A-Fa-f]+|[0-9A-Fa-f]+/),
    start_machine_architecture: _ => token(choice(ci("x86"), ci("amd64"), ci("arm"), ci("arm64"))),

    copy_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("copy")), $.keyword)),
      repeat(field("option", $.copy_option)),
      field("source", $.copy_source_list),
      optional(field("destination", $.copy_destination)),
    )),

    copy_option: $ => choice(
      $.copy_global_flag,
      $.copy_file_mode,
    ),
    copy_global_flag: _ => token(choice(
      ci("/d"),
      ci("/v"),
      ci("/n"),
      ci("/y"),
      ci("/-y"),
      ci("/z"),
      ci("/l"),
    )),
    copy_file_mode: _ => token(choice(ci("/a"), ci("/b"))),
    copy_source_list: $ => prec.right(seq(
      $.copy_source_spec,
      repeat(seq("+", $.copy_source_spec)),
    )),
    copy_source_spec: $ => seq(
      field("path", $.copy_path_argument),
      optional(field("mode", $.copy_file_mode)),
    ),
    copy_destination: $ => seq(
      field("path", $.copy_path_argument),
      optional(field("mode", $.copy_file_mode)),
    ),

    move_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("move")), $.keyword)),
      repeat(field("option", $.move_option)),
      field("source", $.move_source),
      field("destination", $.move_destination),
    )),

    move_option: _ => token(choice(ci("/y"), ci("/-y"))),
    move_source: $ => choice(
      $.move_source_list,
      $.move_source_item,
    ),
    move_source_list: $ => prec.right(seq(
      $.move_source_item,
      repeat1(seq(",", $.move_source_item)),
    )),
    move_source_item: $ => $.move_path_argument,
    move_destination: $ => choice(
      $.move_path_destination,
      $.move_directory_destination,
    ),
    move_path_destination: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.move_file_destination_text,
    ),
    move_directory_destination: $ => $.move_directory_destination_text,

    xcopy_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("xcopy")), $.keyword)),
      field("source", $.xcopy_argument),
      optional(field("destination", $.xcopy_argument)),
      repeat(field("option", $.xcopy_option)),
    )),

    xcopy_option: $ => choice(
      $.xcopy_archive_option,
      $.xcopy_date_option,
      $.xcopy_exclude_option,
      $.xcopy_sparse_option,
      $.xcopy_flag,
    ),
    xcopy_archive_option: _ => token(choice(ci("/a"), ci("/m"))),
    xcopy_date_option: _ => token(/\/[Dd](?::[^ \t\r\n]+)?/),
    xcopy_exclude_option: _ => token(/\/[Ee][Xx][Cc][Ll][Uu][Dd][Ee]:[^ \t\r\n+]+(?:\+[^ \t\r\n+]+)*/),
    xcopy_sparse_option: _ => token(/\/-?[Ss][Pp][Aa][Rr][Ss][Ee]/),
    xcopy_flag: _ => token(choice(
      ci("/p"),
      ci("/s"),
      ci("/e"),
      ci("/v"),
      ci("/w"),
      ci("/c"),
      ci("/i"),
      ci("/-i"),
      ci("/q"),
      ci("/f"),
      ci("/l"),
      ci("/g"),
      ci("/h"),
      ci("/r"),
      ci("/t"),
      ci("/u"),
      ci("/k"),
      ci("/n"),
      ci("/o"),
      ci("/x"),
      ci("/y"),
      ci("/-y"),
      ci("/z"),
      ci("/b"),
      ci("/j"),
      ci("/compress"),
      ci("/noclone"),
    )),

    del_statement: $ => prec.right(seq(
      field("keyword", alias(token(choice(ci("del"), ci("erase"))), $.keyword)),
      repeat(field("option", $.del_option)),
      repeat1(field("target", $.del_target)),
    )),

    del_option: $ => choice(
      $.del_flag,
      $.del_attribute_switch,
    ),
    del_flag: _ => token(choice(ci("/p"), ci("/f"), ci("/s"), ci("/q"))),
    del_attribute_switch: _ => token(/\/[Aa](?::?[-A-Za-z]+)?/),
    del_target: $ => $.move_path_argument,

    rename_statement: $ => prec.right(seq(
      field("keyword", alias(token(choice(ci("ren"), ci("rename"))), $.keyword)),
      field("source", $.rename_source),
      field("target", $.rename_target),
    )),

    rename_source: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.rename_source_text,
    ),
    rename_target: $ => choice(
      $.quoted_string,
      $.rename_target_text,
    ),

    more_file_list_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("more")), $.keyword)),
      repeat(field("option", $.more_option)),
      repeat1(field("file", $.more_file_argument)),
    )),

    more_stream_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("more")), $.keyword)),
      repeat(field("option", $.more_option)),
    )),

    more_option: $ => choice(
      $.more_flag,
      $.more_tabsize_option,
      $.more_starting_line,
    ),
    more_flag: _ => token(choice(ci("/e"), ci("/c"), ci("/p"), ci("/s"))),
    more_tabsize_option: _ => token(/\/[Tt][0-9]+/),
    more_starting_line: _ => token(/\+[0-9]+/),
    more_file_argument: $ => $.move_path_argument,

    find_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("find")), $.keyword)),
      repeat(field("option", $.find_flag)),
      field("pattern", $.quoted_string),
      repeat(field("file", $.find_file_argument)),
    )),

    find_flag: _ => token(choice(
      ci("/v"),
      ci("/c"),
      ci("/n"),
      ci("/i"),
      /\/[Oo][Ff][Ff](?:[Ll][Ii][Nn][Ee])?/,
    )),
    find_file_argument: $ => $.move_path_argument,

    mklink_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("mklink")), $.keyword)),
      optional(field("mode", $.mklink_mode)),
      field("link", $.mklink_path_argument),
      field("target", $.mklink_path_argument),
    )),

    mklink_mode: _ => token(choice(ci("/d"), ci("/h"), ci("/j"))),

    dir_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("dir")), $.keyword)),
      repeat(field("part", choice(
        $.dir_target,
        $.dir_attribute_switch,
        $.dir_order_switch,
        $.dir_time_switch,
        $.dir_flag,
      ))),
    )),

    dir_target: $ => $.command_argument,
    dir_attribute_switch: _ => token(/\/[Aa](?::?[-A-Za-z]+)?/),
    dir_order_switch: _ => token(/\/[Oo](?::?[-A-Za-z]+)?/),
    dir_time_switch: _ => token(/\/[Tt](?::?[CAWcaw])?/),
    dir_flag: _ => token(choice(
      ci("/b"),
      ci("/c"),
      ci("/-c"),
      ci("/d"),
      ci("/l"),
      ci("/n"),
      ci("/p"),
      ci("/q"),
      ci("/r"),
      ci("/s"),
      ci("/w"),
      ci("/x"),
      ci("/4"),
    )),

    findstr_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("findstr")), $.keyword)),
      choice(
        $.findstr_explicit_search_tail,
        $.findstr_default_search_tail,
      ),
    )),

    findstr_explicit_search_tail: $ => prec(1, seq(
      repeat(field("option", $.findstr_non_search_option)),
      repeat1(field("option", $.findstr_search_option)),
      repeat(field("option", $.findstr_non_search_option)),
      repeat(field("file", $.findstr_file_argument)),
    )),
    findstr_default_search_tail: $ => seq(
      repeat(field("option", $.findstr_non_search_option)),
      field("pattern", $.findstr_default_pattern),
      repeat(field("pattern", $.findstr_default_pattern)),
      repeat(field("file", $.findstr_default_file_argument)),
    ),

    findstr_non_search_option: $ => choice(
      $.findstr_flag,
      $.findstr_color_option,
      $.findstr_file_list_option,
      $.findstr_directory_option,
      $.findstr_quiet_option,
    ),
    findstr_search_option: $ => choice(
      $.findstr_search_list_option,
      $.findstr_literal_option,
    ),

    findstr_flag: _ => token(choice(
      ci("/b"),
      ci("/e"),
      ci("/l"),
      ci("/r"),
      ci("/s"),
      ci("/i"),
      ci("/x"),
      ci("/v"),
      ci("/n"),
      ci("/m"),
      ci("/o"),
      ci("/p"),
      /\/[Oo][Ff][Ff](?:[Ll][Ii][Nn][Ee])?/,
    )),
    findstr_color_option: _ => token(/\/[Aa]:[0-9A-Fa-f]+/),
    findstr_file_list_option: _ => token(/\/[Ff]:[^ \t\r\n]+/),
    findstr_search_list_option: _ => token(/\/[Gg]:[^ \t\r\n]+/),
    findstr_directory_option: _ => token(/\/[Dd]:[^ \t\r\n]+/),
    findstr_literal_option: _ => token(/\/[Cc]:(?:"[^"\r\n]*"|[^ \t\r\n]+)/),
    findstr_quiet_option: _ => token(/\/[Qq]:[^ \t\r\n]+/),
    findstr_default_pattern: $ => choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.findstr_default_pattern_text,
    ),
    findstr_pattern: $ => choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.findstr_pattern_text,
    ),
    findstr_default_file_argument: $ => choice(
      $.path,
      $.findstr_default_file_text,
    ),
    findstr_file_argument: $ => choice(
      $.path,
      $.quoted_string,
      $.findstr_file_text,
    ),
    findstr_default_pattern_text: _ => token(/[A-Za-z0-9_^$\[\]()+|!-][A-Za-z0-9_^$\[\]()+|!-]*/),
    findstr_default_file_text: _ => token(choice(
      /[A-Za-z]:[^ \t\r\n]*/,
      /[^ \t\r\n]*\\[^ \t\r\n]*/,
      /[^ \t\r\n]*\.[A-Za-z0-9*?_-]+/,
    )),
    findstr_pattern_text: _ => token(/[^ \t\r\n"'`%][^ \t\r\n]*/),
    findstr_file_text: _ => token(/[A-Za-z0-9_.*?\\/:.-]+/),

    tree_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("tree")), $.keyword)),
      optional(field("target", $.tree_target)),
      repeat(field("option", $.tree_flag)),
    )),

    tree_flag: _ => token(choice(ci("/f"), ci("/a"))),
    tree_target: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.tree_path_text,
    ),

    chcp_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("chcp")), $.keyword)),
      optional(field("code_page", $.chcp_code_page)),
    )),

    chcp_code_page: _ => token(/[0-9]+/),

    subst_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("subst")), $.keyword)),
      optional(field("tail", choice(
        $.subst_assignment,
        $.subst_delete,
      ))),
    )),

    subst_assignment: $ => seq(
      field("drive", $.subst_drive),
      field("path", $.subst_path_argument),
    ),
    subst_delete: $ => seq(
      field("drive", $.subst_drive),
      field("flag", $.subst_delete_flag),
    ),
    subst_drive: _ => token(/[A-Za-z]:/),
    subst_delete_flag: _ => token(ci("/d")),
    subst_path_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.subst_path_text,
    ),

    sort_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("sort")), $.keyword)),
      repeat(field("option", $.sort_option)),
      optional(field("input", $.sort_file_argument)),
      optional(field("temporary", $.sort_temp_option)),
      optional(field("output", $.sort_output_option)),
    )),

    sort_option: $ => choice(
      $.sort_reverse_flag,
      $.sort_column_option,
      $.sort_memory_option,
      $.sort_locale_option,
      $.sort_record_option,
    ),
    sort_reverse_flag: _ => token(/\/[Rr](?:[Ee][Vv][Ee][Rr][Ss][Ee])?/),
    sort_column_option: _ => token(/\/\+[0-9]+/),
    sort_memory_option: $ => seq(
      field("flag", $.sort_memory_flag),
      field("value", $.number),
    ),
    sort_memory_flag: _ => token(/\/[Mm](?:[Ee][Mm][Oo][Rr][Yy])?/),
    sort_locale_option: $ => seq(
      field("flag", $.sort_locale_flag),
      field("value", choice(
        $.quoted_string,
        $.sort_locale_text,
      )),
    ),
    sort_locale_flag: _ => token(/\/[Ll](?:[Oo][Cc][Aa][Ll][Ee])?/),
    sort_record_option: $ => seq(
      field("flag", $.sort_record_flag),
      field("value", $.number),
    ),
    sort_record_flag: _ => token(/\/[Rr][Ee][Cc](?:[Oo][Rr][Dd](?:_[Mm][Aa][Xx][Ii][Mm][Uu][Mm])?)?/),
    sort_temp_option: $ => seq(
      field("flag", $.sort_temp_flag),
      optional(field("directory", $.sort_directory_argument)),
    ),
    sort_temp_flag: _ => token(/\/[Tt](?:[Ee][Mm][Pp][Oo][Rr][Aa][Rr][Yy])?/),
    sort_output_option: $ => seq(
      field("flag", $.sort_output_flag),
      field("file", $.sort_file_argument),
    ),
    sort_output_flag: _ => token(/\/[Oo](?:[Uu][Tt][Pp][Uu][Tt])?/),
    sort_file_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.sort_file_text,
    ),
    sort_directory_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.sort_directory_text,
    ),

    fc_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("fc")), $.keyword)),
      field("tail", choice(
        $.fc_binary_tail,
        $.fc_text_tail,
      )),
    )),

    fc_binary_tail: $ => seq(
      field("mode", $.fc_binary_flag),
      field("left", $.fc_file_argument),
      field("right", $.fc_file_argument),
    ),
    fc_text_tail: $ => seq(
      repeat(field("option", $.fc_text_option)),
      field("left", $.fc_file_argument),
      field("right", $.fc_file_argument),
    ),
    fc_text_option: $ => choice(
      $.fc_text_flag,
      $.fc_line_buffer_option,
      $.fc_resync_option,
    ),
    fc_binary_flag: _ => token(ci("/b")),
    fc_text_flag: _ => token(choice(
      ci("/a"),
      ci("/c"),
      ci("/l"),
      ci("/n"),
      /\/[Oo][Ff][Ff](?:[Ll][Ii][Nn][Ee])?/,
      ci("/t"),
      ci("/u"),
      ci("/w"),
    )),
    fc_line_buffer_option: _ => token(/\/[Ll][Bb][0-9]+/),
    fc_resync_option: _ => token(/\/[0-9]+/),
    fc_file_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.fc_file_text,
    ),

    comp_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("comp")), $.keyword)),
      field("left", $.comp_file_argument),
      field("right", $.comp_file_argument),
      repeat(field("option", $.comp_option)),
    )),

    comp_option: $ => choice(
      $.comp_flag,
      $.comp_limit_option,
    ),
    comp_flag: _ => token(choice(
      ci("/d"),
      ci("/a"),
      ci("/l"),
      ci("/c"),
      /\/[Oo][Ff][Ff](?:[Ll][Ii][Nn][Ee])?/,
      ci("/m"),
    )),
    comp_limit_option: _ => token(/\/[Nn]=[0-9]+/),
    comp_file_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.comp_file_text,
    ),

    mode_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("mode")), $.keyword)),
      optional(field("tail", choice(
        $.mode_redirect_tail,
        $.mode_con_cp_select_tail,
        $.mode_con_cp_status_tail,
        $.mode_con_display_tail,
        $.mode_con_keyboard_tail,
        $.mode_serial_tail,
        $.mode_status_tail,
      ))),
    )),

    mode_redirect_tail: $ => seq(
      field("target", $.mode_lpt_port),
      "=",
      field("source", $.mode_serial_port),
    ),
    mode_con_cp_select_tail: $ => seq(
      optional(field("device", $.mode_console_device)),
      field("keyword", $.mode_cp_keyword),
      field("option", $.mode_cp_select_option),
    ),
    mode_con_cp_status_tail: $ => seq(
      optional(field("device", $.mode_console_device)),
      field("keyword", $.mode_cp_keyword),
      optional(field("flag", $.mode_status_flag)),
    ),
    mode_con_display_tail: $ => seq(
      optional(field("device", $.mode_console_device)),
      repeat1(field("option", choice(
        $.mode_columns_option,
        $.mode_lines_option,
      ))),
    ),
    mode_con_keyboard_tail: $ => seq(
      optional(field("device", $.mode_console_device)),
      repeat1(field("option", choice(
        $.mode_rate_option,
        $.mode_delay_option,
      ))),
    ),
    mode_serial_tail: $ => prec(1, seq(
      field("device", $.mode_serial_port),
      repeat(field("option", $.mode_serial_option)),
    )),
    mode_status_tail: $ => seq(
      choice(
        field("flag", $.mode_status_flag),
        seq(
          field("device", $.mode_known_device),
          optional(field("flag", $.mode_status_flag)),
        ),
      ),
    ),

    mode_serial_option: $ => choice(
      $.mode_baud_option,
      $.mode_parity_option,
      $.mode_data_option,
      $.mode_stop_option,
      $.mode_flow_option,
      $.mode_dtr_option,
      $.mode_rts_option,
    ),
    mode_known_device: $ => choice(
      $.mode_console_device,
      $.mode_serial_port,
      $.mode_lpt_port,
    ),
    mode_console_device: _ => token(/[Cc][Oo][Nn]:?/),
    mode_serial_port: _ => token(/[Cc][Oo][Mm][0-9]+:?/),
    mode_lpt_port: _ => token(/[Ll][Pp][Tt][0-9]+:?/),
    mode_cp_keyword: _ => token(ci("cp")),
    mode_status_flag: _ => token(/\/[Ss][Tt][Aa][Tt][Uu][Ss]/),
    mode_cp_select_option: _ => token(/[Ss][Ee][Ll][Ee][Cc][Tt]=[0-9]+/),
    mode_columns_option: _ => token(/[Cc][Oo][Ll][Ss]=[0-9]+/),
    mode_lines_option: _ => token(/[Ll][Ii][Nn][Ee][Ss]=[0-9]+/),
    mode_rate_option: _ => token(/[Rr][Aa][Tt][Ee]=[0-9]+/),
    mode_delay_option: _ => token(/[Dd][Ee][Ll][Aa][Yy]=[0-9]+/),
    mode_baud_option: _ => token(/[Bb][Aa][Uu][Dd]=[^ \t\r\n]+/),
    mode_parity_option: _ => token(/[Pp][Aa][Rr][Ii][Tt][Yy]=[^ \t\r\n]+/),
    mode_data_option: _ => token(/[Dd][Aa][Tt][Aa]=[^ \t\r\n]+/),
    mode_stop_option: _ => token(/[Ss][Tt][Oo][Pp]=[^ \t\r\n]+/),
    mode_flow_option: _ => token(choice(
      /[Tt][Oo]=(?:[Oo][Nn]|[Oo][Ff][Ff])/,
      /[Xx][Oo][Nn]=(?:[Oo][Nn]|[Oo][Ff][Ff])/,
      /[Oo][Dd][Ss][Rr]=(?:[Oo][Nn]|[Oo][Ff][Ff])/,
      /[Oo][Cc][Tt][Ss]=(?:[Oo][Nn]|[Oo][Ff][Ff])/,
      /[Ii][Dd][Ss][Rr]=(?:[Oo][Nn]|[Oo][Ff][Ff])/,
    )),
    mode_dtr_option: _ => token(/[Dd][Tt][Rr]=(?:[Oo][Nn]|[Oo][Ff][Ff]|[Hh][Ss])/),
    mode_rts_option: _ => token(/[Rr][Tt][Ss]=(?:[Oo][Nn]|[Oo][Ff][Ff]|[Hh][Ss]|[Tt][Gg])/),

    doskey_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("doskey")), $.keyword)),
      repeat(field("option", $.doskey_option)),
      optional(field("macro", $.doskey_macro_definition)),
    )),

    doskey_option: $ => choice(
      $.doskey_flag,
      $.doskey_listsize_option,
      $.doskey_macros_option,
      $.doskey_exename_option,
      $.doskey_macrofile_option,
    ),
    doskey_flag: _ => token(choice(
      ci("/reinstall"),
      ci("/history"),
      ci("/insert"),
      ci("/overstrike"),
    )),
    doskey_listsize_option: _ => token(/\/[Ll][Ii][Ss][Tt][Ss][Ii][Zz][Ee]=[0-9]+/),
    doskey_macros_option: _ => token(/\/[Mm][Aa][Cc][Rr][Oo][Ss](?::(?:[Aa][Ll][Ll]|[^ \t\r\n]+))?/),
    doskey_exename_option: _ => token(/\/[Ee][Xx][Ee][Nn][Aa][Mm][Ee]=[^ \t\r\n]+/),
    doskey_macrofile_option: _ => token(/\/[Mm][Aa][Cc][Rr][Oo][Ff][Ii][Ll][Ee]=[^ \t\r\n]+/),
    doskey_macro_definition: $ => prec.right(seq(
      field("name", $.doskey_macro_name),
      "=",
      optional(field("text", $.doskey_macro_text)),
    )),
    doskey_macro_name: _ => token(/[^ \t\r\n&|<>()"'`%=:]+/),
    doskey_macro_text: $ => prec.right(repeat1(choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.for_variable,
      $.path,
      $.number,
      $.escaped_character,
      $.switch,
      $.doskey_macro_text_fragment,
    ))),

    tasklist_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("tasklist")), $.keyword)),
      optional(field("remote", $.remote_connection_clause)),
      optional(field("mode", $.tasklist_mode_option)),
      repeat(field("filter", $.task_filter_option)),
      optional(field("format", $.table_list_csv_format_option)),
      optional(field("headers", $.no_header_flag)),
    )),

    tasklist_mode_option: $ => choice(
      $.tasklist_module_option,
      $.tasklist_service_flag,
      $.tasklist_apps_flag,
      $.tasklist_verbose_flag,
    ),
    tasklist_module_option: $ => seq(
      field("flag", $.tasklist_module_flag),
      optional(field("module", $.task_value_argument)),
    ),
    tasklist_module_flag: _ => token(ci("/m")),
    tasklist_service_flag: _ => token(ci("/svc")),
    tasklist_apps_flag: _ => token(ci("/apps")),
    tasklist_verbose_flag: _ => token(ci("/v")),

    taskkill_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("taskkill")), $.keyword)),
      optional(field("remote", $.remote_connection_clause)),
      repeat(field("part", choice(
        $.taskkill_selector_option,
        $.taskkill_flag,
      ))),
    )),

    taskkill_selector_option: $ => choice(
      $.task_filter_option,
      $.taskkill_pid_option,
      $.taskkill_image_option,
    ),
    taskkill_pid_option: $ => seq(
      field("flag", $.taskkill_pid_flag),
      field("value", $.task_value_argument),
    ),
    taskkill_pid_flag: _ => token(ci("/pid")),
    taskkill_image_option: $ => seq(
      field("flag", $.taskkill_image_flag),
      field("value", $.task_value_argument),
    ),
    taskkill_image_flag: _ => token(ci("/im")),
    taskkill_flag: _ => token(choice(ci("/t"), ci("/f"))),

    systeminfo_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("systeminfo")), $.keyword)),
      optional(field("remote", $.remote_connection_clause)),
      optional(field("format", $.table_list_csv_format_option)),
      optional(field("headers", $.no_header_flag)),
    )),

    remote_connection_clause: $ => seq(
      field("system", $.remote_system_option),
      optional(field("user", $.remote_user_option)),
      optional(field("password", $.remote_password_option)),
    ),
    remote_system_option: $ => seq(
      field("flag", $.remote_system_flag),
      field("value", $.task_value_argument),
    ),
    remote_system_flag: _ => token(ci("/s")),
    remote_user_option: $ => seq(
      field("flag", $.remote_user_flag),
      field("value", $.task_value_argument),
    ),
    remote_user_flag: _ => token(ci("/u")),
    remote_password_option: $ => seq(
      field("flag", $.remote_password_flag),
      optional(field("value", $.task_value_argument)),
    ),
    remote_password_flag: _ => token(ci("/p")),

    task_filter_option: $ => seq(
      field("flag", $.task_filter_flag),
      field("value", $.task_value_argument),
    ),
    task_filter_flag: _ => token(ci("/fi")),
    table_list_csv_format_option: $ => seq(
      field("flag", $.task_format_flag),
      field("value", $.task_format_value),
    ),
    task_format_flag: _ => token(ci("/fo")),
    task_format_value: $ => choice(
      $.quoted_string,
      $.task_format_text,
    ),
    no_header_flag: _ => token(ci("/nh")),
    task_value_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.task_value_text,
    ),

    shutdown_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("shutdown")), $.keyword)),
      repeat(field("part", $.shutdown_part)),
    )),

    shutdown_part: $ => choice(
      $.shutdown_action_flag,
      $.shutdown_aux_flag,
      $.shutdown_target_option,
      $.shutdown_timeout_option,
      $.shutdown_reason_option,
      $.shutdown_comment_option,
      $.shutdown_help_flag,
    ),
    shutdown_action_flag: _ => token(choice(
      ci("/i"),
      ci("/l"),
      ci("/s"),
      ci("/sg"),
      ci("/r"),
      ci("/g"),
      ci("/a"),
      ci("/p"),
      ci("/h"),
      ci("/e"),
      ci("/o"),
    )),
    shutdown_aux_flag: _ => token(choice(
      ci("/hybrid"),
      ci("/soft"),
      ci("/fw"),
      ci("/f"),
    )),
    shutdown_target_option: $ => seq(
      field("flag", $.shutdown_target_flag),
      field("target", $.shutdown_target_argument),
    ),
    shutdown_target_flag: _ => token(ci("/m")),
    shutdown_target_argument: $ => choice(
      $.path,
      $.quoted_string,
      $.task_value_text,
    ),
    shutdown_timeout_option: $ => seq(
      field("flag", $.shutdown_timeout_flag),
      field("value", $.number),
    ),
    shutdown_timeout_flag: _ => token(ci("/t")),
    shutdown_reason_option: $ => seq(
      field("flag", $.shutdown_reason_flag),
      field("value", $.shutdown_reason_value),
    ),
    shutdown_reason_flag: _ => token(ci("/d")),
    shutdown_reason_value: _ => token(/(?:[Pp]|[Uu]):[0-9]+:[0-9]+|[0-9]+:[0-9]+/),
    shutdown_comment_option: $ => seq(
      field("flag", $.shutdown_comment_flag),
      field("value", $.quoted_string),
    ),
    shutdown_comment_flag: _ => token(ci("/c")),
    shutdown_help_flag: _ => token(ci("/?")),

    driverquery_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("driverquery")), $.keyword)),
      optional(field("remote", $.remote_connection_clause)),
      optional(field("format", $.table_list_csv_format_option)),
      optional(field("headers", $.no_header_flag)),
      repeat(field("detail", $.driverquery_detail_flag)),
    )),

    driverquery_detail_flag: _ => token(choice(ci("/si"), ci("/v"), ci("/?"))),

    openfiles_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("openfiles")), $.keyword)),
      optional(field("mode", $.openfiles_mode_flag)),
      optional(field("help", $.openfiles_help_flag)),
    )),

    openfiles_mode_flag: _ => token(choice(ci("/disconnect"), ci("/query"), ci("/local"))),
    openfiles_help_flag: _ => token(ci("/?")),

    schtasks_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("schtasks")), $.keyword)),
      optional(field("mode", $.schtasks_mode_flag)),
      optional(field("help", $.schtasks_help_flag)),
    )),

    schtasks_mode_flag: _ => token(choice(
      ci("/create"),
      ci("/delete"),
      ci("/query"),
      ci("/change"),
      ci("/run"),
      ci("/end"),
      ci("/showsid"),
    )),
    schtasks_help_flag: _ => token(ci("/?")),

    sc_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("sc")), $.keyword)),
      optional(field("server", $.sc_server)),
      optional(field("tail", choice(
        $.sc_query_tail,
        $.sc_service_command_tail,
        $.sc_manager_command_tail,
      ))),
    )),

    sc_query_tail: $ => choice(
      prec(1, seq(
        field("command", $.sc_query_command),
        field("target", $.sc_service_name),
      )),
      seq(
        field("command", $.sc_query_command),
        repeat1(field("option", $.sc_query_option)),
      ),
      seq(
        field("command", $.sc_query_command),
      ),
    ),
    sc_query_command: _ => token(choice(ci("query"), ci("queryex"))),
    sc_query_option: $ => choice(
      $.sc_type_option,
      $.sc_state_option,
      $.sc_bufsize_option,
      $.sc_resume_index_option,
      $.sc_group_option,
    ),
    sc_type_option: $ => seq(
      field("flag", $.sc_type_flag),
      field("value", $.sc_argument),
    ),
    sc_type_flag: _ => token(/[Tt][Yy][Pp][Ee]=/),
    sc_state_option: $ => seq(
      field("flag", $.sc_state_flag),
      field("value", $.sc_argument),
    ),
    sc_state_flag: _ => token(/[Ss][Tt][Aa][Tt][Ee]=/),
    sc_bufsize_option: $ => seq(
      field("flag", $.sc_bufsize_flag),
      field("value", $.number),
    ),
    sc_bufsize_flag: _ => token(/[Bb][Uu][Ff][Ss][Ii][Zz][Ee]=/),
    sc_resume_index_option: $ => seq(
      field("flag", $.sc_resume_index_flag),
      field("value", $.number),
    ),
    sc_resume_index_flag: _ => token(/[Rr][Ii]=/),
    sc_group_option: $ => seq(
      field("flag", $.sc_group_flag),
      field("value", $.sc_argument),
    ),
    sc_group_flag: _ => token(/[Gg][Rr][Oo][Uu][Pp]=/),

    sc_service_command_tail: $ => prec.right(seq(
      field("command", $.sc_service_command),
      field("target", $.sc_service_name),
      repeat(field("argument", $.sc_argument)),
    )),
    sc_service_command: _ => token(choice(
      ci("start"),
      ci("pause"),
      ci("interrogate"),
      ci("continue"),
      ci("stop"),
      ci("config"),
      ci("description"),
      ci("failure"),
      ci("failureflag"),
      ci("sidtype"),
      ci("privs"),
      ci("managedaccount"),
      ci("qc"),
      ci("qdescription"),
      ci("qfailure"),
      ci("qfailureflag"),
      ci("qsidtype"),
      ci("qprivs"),
      ci("qtriggerinfo"),
      ci("qpreferrednode"),
      ci("qmanagedaccount"),
      ci("qprotection"),
      ci("quserservice"),
      ci("delete"),
      ci("create"),
      ci("control"),
      ci("sdshow"),
      ci("sdset"),
      ci("showsid"),
      ci("triggerinfo"),
      ci("preferrednode"),
      ci("getdisplayname"),
      ci("getkeyname"),
      ci("enumdepend"),
    )),

    sc_manager_command_tail: $ => seq(
      choice(
        seq(
          field("command", $.sc_boot_command),
          field("argument", $.sc_argument),
        ),
        seq(
          field("command", $.sc_lock_command),
        ),
      ),
    ),
    sc_boot_command: _ => token(ci("boot")),
    sc_lock_command: _ => token(choice(ci("lock"), ci("querylock"))),
    sc_server: _ => token(/\\\\[^ \t\r\n]+/),
    sc_service_name: $ => $.sc_argument,
    sc_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.number,
      $.sc_text,
    ),

    gpresult_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("gpresult")), $.keyword)),
      optional(field("remote", $.remote_connection_clause)),
      optional(field("scope", $.gpresult_scope_option)),
      optional(field("user", $.gpresult_target_user_option)),
      optional(field("mode", $.gpresult_mode_option)),
      optional(field("report", $.gpresult_report_option)),
      optional(field("force", $.gpresult_force_flag)),
    )),

    gpresult_scope_option: $ => seq(
      field("flag", $.gpresult_scope_flag),
      field("value", $.gpresult_scope_value),
    ),
    gpresult_scope_flag: _ => token(ci("/scope")),
    gpresult_scope_value: _ => token(choice(ci("user"), ci("computer"))),
    gpresult_target_user_option: $ => seq(
      field("flag", $.gpresult_user_flag),
      field("value", $.task_value_argument),
    ),
    gpresult_user_flag: _ => token(ci("/user")),
    gpresult_mode_option: _ => token(choice(ci("/r"), ci("/v"), ci("/z"), ci("/?"))),
    gpresult_report_option: $ => choice(
      $.gpresult_xml_report,
      $.gpresult_html_report,
    ),
    gpresult_xml_report: $ => seq(
      field("flag", $.gpresult_xml_flag),
      field("file", $.gpresult_report_file),
    ),
    gpresult_html_report: $ => seq(
      field("flag", $.gpresult_html_flag),
      field("file", $.gpresult_report_file),
    ),
    gpresult_xml_flag: _ => token(ci("/x")),
    gpresult_html_flag: _ => token(ci("/h")),
    gpresult_force_flag: _ => token(ci("/f")),
    gpresult_report_file: $ => choice(
      $.quoted_string,
      $.path,
      $.task_value_text,
    ),

    bcdedit_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("bcdedit")), $.keyword)),
      optional(field("store", $.bcdedit_store_option)),
      optional(field("command", $.bcdedit_command_clause)),
      repeat(field("flag", $.bcdedit_flag)),
    )),

    bcdedit_store_option: $ => seq(
      field("flag", $.bcdedit_store_flag),
      field("value", $.bcdedit_argument),
    ),
    bcdedit_store_flag: _ => token(ci("/store")),
    bcdedit_command_clause: $ => prec.right(seq(
      field("command", $.bcdedit_command),
      repeat(field("argument", $.bcdedit_argument)),
    )),
    bcdedit_command: _ => token(choice(
      ci("/createstore"),
      ci("/export"),
      ci("/import"),
      ci("/sysstore"),
      ci("/copy"),
      ci("/create"),
      ci("/delete"),
      ci("/mirror"),
      ci("/deletevalue"),
      ci("/set"),
      ci("/enum"),
      ci("/bootsequence"),
      ci("/default"),
      ci("/displayorder"),
      ci("/timeout"),
      ci("/toolsdisplayorder"),
      ci("/bootems"),
      ci("/ems"),
      ci("/emssettings"),
      ci("/bootdebug"),
      ci("/dbgsettings"),
      ci("/debug"),
      ci("/hypervisorsettings"),
      ci("/eventsettings"),
      ci("/event"),
    )),
    bcdedit_flag: _ => token(choice(ci("/v"), ci("/?"))),
    bcdedit_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.number,
      $.bcdedit_identifier,
      $.bcdedit_text,
    ),
    bcdedit_identifier: _ => token(/\{[^\r\n\}]+\}/),

    compact_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("compact")), $.keyword)),
      optional(field("tail", choice(
        $.compact_general_tail,
        $.compact_os_tail,
      ))),
    )),

    compact_general_tail: $ => choice(
      seq(
        repeat1(field("option", $.compact_general_option)),
        repeat(field("target", $.compact_target_argument)),
      ),
      seq(
        repeat(field("option", $.compact_general_option)),
        repeat1(field("target", $.compact_target_argument)),
      ),
    ),
    compact_general_option: $ => choice(
      $.compact_mode_flag,
      $.compact_recursive_option,
      $.compact_flag,
      $.compact_exe_option,
    ),
    compact_mode_flag: _ => token(choice(ci("/c"), ci("/u"))),
    compact_recursive_option: _ => token(/\/[Ss](?::[^ \t\r\n]+)?/),
    compact_flag: _ => token(choice(ci("/a"), ci("/i"), ci("/f"), ci("/q"))),
    compact_exe_option: _ => token(/\/[Ee][Xx][Ee](?::[^ \t\r\n]+)?/),
    compact_os_tail: $ => seq(
      field("option", $.compact_os_option),
      optional(field("windir", $.compact_windir_option)),
    ),
    compact_os_option: _ => token(/\/[Cc][Oo][Mm][Pp][Aa][Cc][Tt][Oo][Ss](?::(?:[Qq][Uu][Ee][Rr][Yy]|[Aa][Ll][Ww][Aa][Yy][Ss]|[Nn][Ee][Vv][Ee][Rr]))?/),
    compact_windir_option: _ => token(/\/[Ww][Ii][Nn][Dd][Ii][Rr]:[^ \t\r\n]+/),
    compact_target_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.compact_target_text,
    ),

    replace_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("replace")), $.keyword)),
      field("source", $.replace_source_argument),
      field("destination", $.replace_destination_argument),
      repeat(field("option", $.replace_flag)),
    )),

    replace_flag: _ => token(choice(ci("/a"), ci("/p"), ci("/r"), ci("/s"), ci("/w"), ci("/u"))),
    replace_source_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.replace_source_text,
    ),
    replace_destination_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.replace_destination_text,
    ),

    convert_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("convert")), $.keyword)),
      field("volume", $.convert_volume_argument),
      field("filesystem", $.convert_filesystem_flag),
      repeat(field("option", $.convert_option)),
    )),

    convert_filesystem_flag: _ => token(/\/[Ff][Ss]:[Nn][Tt][Ff][Ss]/),
    convert_option: _ => token(choice(
      ci("/v"),
      /\/[Cc][Vv][Tt][Aa][Rr][Ee][Aa]:[^ \t\r\n]+/,
      ci("/nosecurity"),
      ci("/x"),
    )),
    convert_volume_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.convert_volume_text,
    ),

    chkdsk_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("chkdsk")), $.keyword)),
      optional(field("target", $.chkdsk_target_argument)),
      repeat(field("option", $.chkdsk_option)),
    )),

    chkdsk_option: _ => token(choice(
      ci("/f"),
      ci("/v"),
      ci("/r"),
      ci("/x"),
      ci("/i"),
      ci("/c"),
      /\/[Ll](?::[0-9]+)?/,
      ci("/b"),
      ci("/scan"),
      ci("/spotfix"),
      ci("/forceofflinefix"),
      ci("/perf"),
      ci("/sdcleanup"),
      ci("/offlinescanandfix"),
      ci("/freeorphanedchains"),
      ci("/markclean"),
    )),
    chkdsk_target_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.chkdsk_target_text,
    ),

    chkntfs_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("chkntfs")), $.keyword)),
      optional(field("tail", choice(
        $.chkntfs_default_flag,
        $.chkntfs_timeout_option,
        $.chkntfs_exclude_clause,
        $.chkntfs_schedule_clause,
        $.chkntfs_volume_clause,
      ))),
    )),

    chkntfs_default_flag: _ => token(ci("/d")),
    chkntfs_timeout_option: _ => token(/\/[Tt](?::[0-9]+)?/),
    chkntfs_exclude_clause: $ => seq(
      field("flag", $.chkntfs_exclude_flag),
      repeat1(field("volume", $.chkntfs_volume_argument)),
    ),
    chkntfs_exclude_flag: _ => token(ci("/x")),
    chkntfs_schedule_clause: $ => seq(
      field("flag", $.chkntfs_schedule_flag),
      repeat1(field("volume", $.chkntfs_volume_argument)),
    ),
    chkntfs_schedule_flag: _ => token(ci("/c")),
    chkntfs_volume_clause: $ => repeat1(field("volume", $.chkntfs_volume_argument)),
    chkntfs_volume_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.chkntfs_volume_text,
    ),

    print_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("print")), $.keyword)),
      optional(field("device", $.print_device_option)),
      repeat(field("file", $.print_file_argument)),
    )),

    print_device_option: _ => token(/\/[Dd]:[^ \t\r\n]+/),
    print_file_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.print_file_text,
    ),

    icacls_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("icacls")), $.keyword)),
      field("target", $.icacls_target_argument),
      field("tail", choice(
        $.icacls_save_tail,
        $.icacls_restore_tail,
        $.icacls_setowner_tail,
        $.icacls_findsid_tail,
        $.icacls_verify_tail,
        $.icacls_reset_tail,
        $.icacls_edit_tail,
      )),
    )),

    icacls_save_tail: $ => seq(
      field("command", $.icacls_save_flag),
      field("aclfile", $.icacls_argument),
      repeat(field("option", $.icacls_common_flag)),
    ),
    icacls_restore_tail: $ => seq(
      optional(repeat1(field("substitute", $.icacls_substitute_clause))),
      field("command", $.icacls_restore_flag),
      field("aclfile", $.icacls_argument),
      repeat(field("option", $.icacls_common_flag)),
    ),
    icacls_setowner_tail: $ => seq(
      field("command", $.icacls_setowner_flag),
      field("owner", $.icacls_argument),
      repeat(field("option", $.icacls_common_flag)),
    ),
    icacls_findsid_tail: $ => seq(
      field("command", $.icacls_findsid_flag),
      field("sid", $.icacls_argument),
      repeat(field("option", $.icacls_common_flag)),
    ),
    icacls_verify_tail: $ => seq(
      field("command", $.icacls_verify_flag),
      repeat(field("option", $.icacls_common_flag)),
    ),
    icacls_reset_tail: $ => seq(
      field("command", $.icacls_reset_flag),
      repeat(field("option", $.icacls_common_flag)),
    ),
    icacls_edit_tail: $ => seq(
      repeat1(field("action", $.icacls_action)),
      repeat(field("option", choice($.icacls_common_flag, $.icacls_inheritance_option))),
    ),
    icacls_action: $ => choice(
      $.icacls_grant_action,
      $.icacls_deny_action,
      $.icacls_remove_action,
      $.icacls_integrity_action,
    ),
    icacls_grant_action: $ => seq(
      field("flag", $.icacls_grant_flag),
      repeat1(field("entry", $.icacls_argument)),
    ),
    icacls_deny_action: $ => seq(
      field("flag", $.icacls_deny_flag),
      repeat1(field("entry", $.icacls_argument)),
    ),
    icacls_remove_action: $ => seq(
      field("flag", $.icacls_remove_flag),
      repeat1(field("entry", $.icacls_argument)),
    ),
    icacls_integrity_action: $ => seq(
      field("flag", $.icacls_integrity_flag),
      repeat1(field("entry", $.icacls_argument)),
    ),
    icacls_substitute_clause: $ => seq(
      field("flag", $.icacls_substitute_flag),
      field("old_sid", $.icacls_argument),
      field("new_sid", $.icacls_argument),
    ),
    icacls_save_flag: _ => token(ci("/save")),
    icacls_restore_flag: _ => token(ci("/restore")),
    icacls_setowner_flag: _ => token(ci("/setowner")),
    icacls_findsid_flag: _ => token(ci("/findsid")),
    icacls_verify_flag: _ => token(ci("/verify")),
    icacls_reset_flag: _ => token(ci("/reset")),
    icacls_grant_flag: _ => token(/\/[Gg][Rr][Aa][Nn][Tt](?::[Rr])?/),
    icacls_deny_flag: _ => token(ci("/deny")),
    icacls_remove_flag: _ => token(/\/[Rr][Ee][Mm][Oo][Vv][Ee](?::[GgDd])?/),
    icacls_integrity_flag: _ => token(ci("/setintegritylevel")),
    icacls_substitute_flag: _ => token(ci("/substitute")),
    icacls_inheritance_option: _ => token(/\/[Ii][Nn][Hh][Ee][Rr][Ii][Tt][Aa][Nn][Cc][Ee]:[EeDdRr]/),
    icacls_common_flag: _ => token(choice(ci("/t"), ci("/c"), ci("/l"), ci("/q"))),
    icacls_target_argument: $ => $.icacls_argument,
    icacls_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.icacls_text,
    ),

    cacls_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("cacls")), $.keyword)),
      field("target", $.cacls_argument),
      repeat(field("part", $.cacls_part)),
    )),

    cacls_part: $ => choice(
      $.cacls_simple_flag,
      $.cacls_sddl_option,
      $.cacls_grant_option,
      $.cacls_remove_option,
      $.cacls_replace_option,
      $.cacls_deny_option,
    ),
    cacls_simple_flag: _ => token(choice(ci("/t"), ci("/m"), ci("/l"), ci("/s"), ci("/e"), ci("/c"))),
    cacls_sddl_option: _ => token(/\/[Ss]:[^ \t\r\n]+/),
    cacls_grant_option: $ => seq(
      field("flag", $.cacls_grant_flag),
      repeat1(field("entry", $.cacls_argument)),
    ),
    cacls_remove_option: $ => seq(
      field("flag", $.cacls_remove_flag),
      repeat1(field("entry", $.cacls_argument)),
    ),
    cacls_replace_option: $ => seq(
      field("flag", $.cacls_replace_flag),
      repeat1(field("entry", $.cacls_argument)),
    ),
    cacls_deny_option: $ => seq(
      field("flag", $.cacls_deny_flag),
      repeat1(field("entry", $.cacls_argument)),
    ),
    cacls_grant_flag: _ => token(ci("/g")),
    cacls_remove_flag: _ => token(ci("/r")),
    cacls_replace_flag: _ => token(ci("/p")),
    cacls_deny_flag: _ => token(ci("/d")),
    cacls_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.cacls_text,
    ),

    recover_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("recover")), $.keyword)),
      field("target", $.recover_target_argument),
    )),

    recover_target_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.recover_target_text,
    ),

    format_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("format")), $.keyword)),
      field("volume", $.format_volume_argument),
      repeat(field("option", $.format_option)),
    )),

    format_option: _ => token(choice(
      /\/[Ff][Ss]:[^ \t\r\n]+/,
      /\/[Vv]:[^ \t\r\n]+/,
      ci("/q"),
      /\/[Ll](?::[^ \t\r\n]+)?/,
      /\/[Aa]:[^ \t\r\n]+/,
      ci("/c"),
      /\/[Ii]:[^ \t\r\n]+/,
      ci("/x"),
      /\/[Pp]:[^ \t\r\n]+/,
      /\/[Ss]:[^ \t\r\n]+/,
      /\/[Rr]:[^ \t\r\n]+/,
      ci("/d"),
      /\/[Ff]:[^ \t\r\n]+/,
      /\/[Tt]:[^ \t\r\n]+/,
      /\/[Nn]:[^ \t\r\n]+/,
      /\/[Tt][Xx][Ff]:[^ \t\r\n]+/,
      /\/[Dd][Aa][Xx](?::[^ \t\r\n]+)?/,
      /\/[Ll][Oo][Gg][Ss][Ii][Zz][Ee](?::[^ \t\r\n]+)?/,
      ci("/norepairlogs"),
      ci("/devdrv"),
      ci("/sha256checksums"),
      ci("/y"),
    )),
    format_volume_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.format_volume_text,
    ),

    fsutil_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("fsutil")), $.keyword)),
      field("command", $.fsutil_command),
      repeat(field("argument", $.fsutil_argument)),
    )),

    fsutil_command: _ => token(choice(
      ci("8dot3name"),
      ci("behavior"),
      ci("bypassio"),
      ci("clfs"),
      ci("dax"),
      ci("devdrv"),
      ci("dirty"),
      ci("file"),
      ci("fsinfo"),
      ci("hardlink"),
      ci("objectid"),
      ci("quota"),
      ci("repair"),
      ci("reparsepoint"),
      ci("storagereserve"),
      ci("resource"),
      ci("sparse"),
      ci("tiering"),
      ci("trace"),
      ci("transaction"),
      ci("usn"),
      ci("volume"),
      ci("wim"),
    )),
    fsutil_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.number,
      $.fsutil_text,
    ),

    label_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("label")), $.keyword)),
      optional(field("tail", choice(
        $.label_mount_tail,
        $.label_drive_tail,
        $.label_value_argument,
      ))),
    )),

    label_mount_tail: $ => seq(
      field("flag", $.label_mount_flag),
      field("target", $.label_target_argument),
      optional(field("label", $.label_value_argument)),
    ),
    label_drive_tail: $ => seq(
      field("target", $.label_drive),
      optional(field("label", $.label_value_argument)),
    ),
    label_mount_flag: _ => token(ci("/mp")),
    label_target_argument: $ => choice(
      $.label_drive,
      $.quoted_string,
      $.path,
    ),
    label_value_argument: $ => choice(
      $.quoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.label_value_text,
    ),

    cmd_statement: $ => prec.right(seq(
      field("keyword", alias(token(choice(ci("cmd"), ci("cmd.exe"))), $.keyword)),
      repeat(field("option", $.cmd_option)),
      optional(field("tail", $.cmd_command_tail)),
    )),

    cmd_option: $ => choice(
      $.cmd_encoding_flag,
      $.cmd_flag,
      $.cmd_color_option,
      $.cmd_state_option,
    ),
    cmd_encoding_flag: _ => token(choice(ci("/a"), ci("/u"))),
    cmd_flag: _ => token(choice(ci("/q"), ci("/d"), ci("/x"), ci("/y"), ci("/r"))),
    cmd_color_option: _ => token(/\/[Tt]:[^ \t\r\n]+/),
    cmd_state_option: _ => token(choice(
      /\/[Ee]:(?:[Oo][Nn]|[Oo][Ff][Ff])/,
      /\/[Ff]:(?:[Oo][Nn]|[Oo][Ff][Ff])/,
      /\/[Vv]:(?:[Oo][Nn]|[Oo][Ff][Ff])/
    )),
    cmd_command_tail: $ => prec.right(seq(
      optional(field("flag", $.cmd_shell_string_flag)),
      field("mode", $.cmd_shell_mode),
      repeat1(field("command", $.command_argument)),
    )),
    cmd_shell_string_flag: _ => token(ci("/s")),
    cmd_shell_mode: _ => token(choice(ci("/c"), ci("/k"))),

    robocopy_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("robocopy")), $.keyword)),
      field("source", $.robocopy_source_argument),
      field("destination", $.robocopy_destination_argument),
      repeat(field("part", $.robocopy_part)),
    )),

    robocopy_part: $ => choice(
      $.robocopy_file_argument,
      $.robocopy_list_option,
      $.robocopy_flag,
      $.robocopy_copy_option,
      $.robocopy_attribute_option,
      $.robocopy_monitor_option,
      $.robocopy_throttle_option,
      $.robocopy_selection_option,
      $.robocopy_retry_option,
      $.robocopy_log_option,
      $.robocopy_job_option,
      $.robocopy_value_option,
    ),
    robocopy_flag: _ => token(choice(
      ci("/s"),
      ci("/e"),
      ci("/z"),
      ci("/b"),
      ci("/zb"),
      ci("/j"),
      ci("/efsraw"),
      ci("/sec"),
      ci("/copyall"),
      ci("/nocopy"),
      ci("/secfix"),
      ci("/timfix"),
      ci("/purge"),
      ci("/mir"),
      ci("/mov"),
      ci("/move"),
      ci("/create"),
      ci("/fat"),
      ci("/256"),
      ci("/pf"),
      ci("/nodcopy"),
      ci("/nooffload"),
      ci("/compress"),
      ci("/noclone"),
      ci("/a"),
      ci("/m"),
      ci("/xc"),
      ci("/xn"),
      ci("/xo"),
      ci("/xx"),
      ci("/xl"),
      ci("/is"),
      ci("/it"),
      ci("/fft"),
      ci("/dst"),
      ci("/xj"),
      ci("/xjd"),
      ci("/xjf"),
      ci("/im"),
      ci("/reg"),
      ci("/tbd"),
      ci("/lfsm"),
      ci("/l"),
      ci("/x"),
      ci("/v"),
      ci("/ts"),
      ci("/fp"),
      ci("/bytes"),
      ci("/ns"),
      ci("/nc"),
      ci("/nfl"),
      ci("/ndl"),
      ci("/np"),
      ci("/eta"),
      ci("/tee"),
      ci("/njh"),
      ci("/njs"),
      ci("/unicode"),
      ci("/quit"),
      ci("/nosd"),
      ci("/nodd"),
    )),
    robocopy_copy_option: $ => seq(
      field("flag", $.robocopy_copy_flag),
      field("value", $.robocopy_copy_value),
    ),
    robocopy_copy_flag: _ => token(choice(ci("/copy:"), ci("/dcopy:"))),
    robocopy_copy_value: _ => token.immediate(/[^ \t\r\n]+/),
    robocopy_attribute_option: $ => seq(
      field("flag", $.robocopy_attribute_flag),
      field("value", $.robocopy_attribute_value),
    ),
    robocopy_attribute_flag: _ => token(choice(ci("/ia:"), ci("/xa:"), ci("/a+:"), ci("/a-:"))),
    robocopy_attribute_value: _ => token.immediate(/[^ \t\r\n]+/),
    robocopy_monitor_option: $ => seq(
      field("flag", $.robocopy_monitor_flag),
      field("value", $.robocopy_monitor_value),
    ),
    robocopy_monitor_flag: _ => token(choice(
      ci("/lev:"),
      ci("/mon:"),
      ci("/mot:"),
      ci("/rh:"),
      ci("/ipg:"),
      ci("/mt:"),
    )),
    robocopy_monitor_value: _ => token.immediate(/[^ \t\r\n]+/),
    robocopy_throttle_option: $ => seq(
      field("flag", $.robocopy_throttle_flag),
      field("value", $.robocopy_throttle_value),
    ),
    robocopy_throttle_flag: _ => token(choice(
      ci("/iomaxsize:"),
      ci("/iorate:"),
      ci("/threshold:"),
      ci("/lfsm:"),
    )),
    robocopy_throttle_value: _ => token.immediate(/[^ \t\r\n]+/),
    robocopy_selection_option: $ => seq(
      field("flag", $.robocopy_selection_flag),
      field("value", $.robocopy_selection_value),
    ),
    robocopy_selection_flag: _ => token(choice(
      ci("/max:"),
      ci("/min:"),
      ci("/maxage:"),
      ci("/minage:"),
      ci("/maxlad:"),
      ci("/minlad:"),
    )),
    robocopy_selection_value: _ => token.immediate(/[^ \t\r\n]+/),
    robocopy_retry_option: $ => seq(
      field("flag", $.robocopy_retry_flag),
      field("value", $.robocopy_retry_value),
    ),
    robocopy_retry_flag: _ => token(choice(
      ci("/r:"),
      ci("/w:"),
    )),
    robocopy_retry_value: _ => token.immediate(/[^ \t\r\n]+/),
    robocopy_log_option: $ => seq(
      field("flag", $.robocopy_log_flag),
      field("value", $.robocopy_log_value),
    ),
    robocopy_log_flag: _ => token(choice(ci("/log:"), ci("/log+:"), ci("/unilog:"), ci("/unilog+:"))),
    robocopy_log_value: _ => token.immediate(/[^ \t\r\n]+/),
    robocopy_job_option: $ => seq(
      field("flag", $.robocopy_job_flag),
      field("value", $.robocopy_job_value),
    ),
    robocopy_job_flag: _ => token(choice(ci("/job:"), ci("/save:"))),
    robocopy_job_value: _ => token.immediate(/[^ \t\r\n]+/),
    robocopy_value_option: _ => token(/\/[Ss][Pp][Aa][Rr][Ss][Ee](?::[^ \t\r\n]+)?/),
    robocopy_list_option: $ => prec.right(seq(
      field("flag", $.robocopy_list_flag),
      repeat1(field("value", $.robocopy_file_argument)),
    )),
    robocopy_list_flag: _ => token(choice(ci("/xf"), ci("/xd"), ci("/if"))),
    robocopy_source_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.robocopy_text,
    ),
    robocopy_destination_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.robocopy_text,
    ),
    robocopy_file_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.robocopy_file_text,
    ),

    echo_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("echo")), $.keyword)),
      optional(field("tail", choice(
        $.echo_mode_clause,
        $.echo_message_clause,
      ))),
    )),

    echo_mode_clause: $ => field("mode", $.echo_mode),
    echo_message_clause: $ => prec.right(seq(
      field("message", $.argument),
      repeat(field("message", $.argument)),
    )),
    echo_mode: _ => token(choice(ci("on"), ci("off"))),

    setlocal_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("setlocal")), $.keyword)),
      optional(repeat1(field("option", $.setlocal_option))),
    )),

    setlocal_option: _ => token(choice(
      ci("enableextensions"),
      ci("disableextensions"),
      ci("enabledelayedexpansion"),
      ci("disabledelayedexpansion"),
    )),

    endlocal_statement: $ => seq(
      field("keyword", alias(token(ci("endlocal")), $.keyword)),
    ),

    help_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("help")), $.keyword)),
      optional(field("topic", $.help_topic)),
    )),

    help_topic: $ => $.word,

    type_statement: $ => prec.right(seq(
      field("keyword", alias(token(ci("type")), $.keyword)),
      repeat1(field("file", $.type_file_argument)),
    )),

    type_file_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.type_file_text,
    ),

    simple_command: $ => prec.right(seq(
      field("name", $.command_name),
      repeat(choice($.switch, $.argument)),
    )),

    redirection: $ => seq(
      optional(field("descriptor", $.number)),
      field("operator", $.redirection_operator),
      field("target", $.argument),
    ),

    redirection_operator: _ => choice(
      "<",
      ">",
      ">>",
    ),

    command_operator: _ => choice(
      "&&",
      "||",
      "&",
      "|",
    ),

    comparison_operator: _ => token(choice(...comparisonOperators.map(ci))),

    label: $ => seq(":", field("name", $.label_name)),
    label_reference: $ => seq(":", field("name", choice($.label_name, alias(token(ci("eof")), $.label_name)))),
    label_name: _ => token(/[A-Za-z0-9_.-]+/),
    call_label_target: $ => seq(":", field("name", $.label_name)),
    call_batch_target: $ => choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.path,
      $.call_batch_text,
    ),
    call_label_argument: $ => $.argument,
    call_batch_argument: $ => $.argument,
    goto_label_target: $ => seq(":", field("name", $.label_name)),
    goto_eof_target: _ => token(ci(":eof")),

    comment: _ => token(choice(
      /::[^\r\n]*/,
      /[Rr][Ee][Mm](?:[ \t].*)?/,
    )),

    command_prefix: _ => "@",

    command_name: $ => choice(
      $.path,
      $.word,
    ),

    command_argument: $ => choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.for_variable,
      $.path,
      $.number,
      $.escaped_character,
      $.argument_text,
    ),

    copy_path_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.copy_path_text,
    ),

    move_path_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.move_path_text,
    ),

    mklink_path_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.mklink_path_text,
    ),

    xcopy_argument: $ => choice(
      $.quoted_string,
      $.path,
      $.variable_expansion,
      $.delayed_variable,
      $.xcopy_path_text,
    ),

    argument: $ => prec.right(repeat1(choice(
      $.quoted_string,
      $.single_quoted_string,
      $.backquoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.for_variable,
      $.path,
      $.number,
      $.escaped_character,
      $.argument_text,
    ))),

    quoted_string: $ => seq(
      '"',
      repeat(choice(
        $.string_content,
        $.escaped_character,
        $.variable_expansion,
        $.delayed_variable,
        $.for_variable,
      )),
      token.immediate('"'),
    ),

    single_quoted_string: _ => token(/'[^'\r\n]*'/),
    backquoted_string: _ => token(/`[^`\r\n]*`/),
    string_content: _ => token.immediate(/[^"%!^\r\n]+/),

    variable_expansion: _ => token(choice(
      /%[A-Za-z_][A-Za-z0-9_]*%/,
      /%\*/,
      /%[0-9]/,
      /%~(?:[fdpnxsatz]+)?(?:\$[A-Za-z_][A-Za-z0-9_]*:)?[0-9A-Za-z]/,
    )),

    delayed_variable: _ => token(/![^!\r\n]+!/),
    for_variable: _ => token(/%%?[A-Za-z]/),
    escaped_character: _ => token(/\^[^\r\n]/),

    switch: _ => token(/\/[A-Za-z0-9?][^ \t\r\n&|<>()"]*/),
    path: _ => token(choice(
      /[A-Za-z]:\\[^ \t\r\n&|<>()"]*/,
      /\\[^ \t\r\n&|<>()"]*/,
      /\.\.\\[^ \t\r\n&|<>()"]*/,
      /\.\\[^ \t\r\n&|<>()"]*/,
    )),
    path_value_segment: _ => token(choice(
      /[A-Za-z]:\\[^; \t\r\n&|<>()"]*/,
      /\\[^; \t\r\n&|<>()"]*/,
      /\.\.\\[^; \t\r\n&|<>()"]*/,
      /\.\\[^; \t\r\n&|<>()"]*/,
      /[^; \t\r\n&|<>()"'`%]+/,
    )),
    path_text_segment: _ => token(/[^; \t\r\n&|<>()"'`%]+/),
    number: _ => token(/[+-]?\d+/),
    cd_path_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    copy_path_text: _ => token(/[^ +\t\r\n&|<>()"'`%]+/),
    move_path_text: _ => token(/[^, \t\r\n&|<>()"'`%]+/),
    move_file_destination_text: _ => token(choice(
      /[^ \\,\t\r\n&|<>()"'`%]*\.[^ \\,\t\r\n&|<>()"'`%]+/,
      /[^ \\,\t\r\n&|<>()"'`%]*[\\/:][^ \\,\t\r\n&|<>()"'`%]*/,
      /[^ \\,\t\r\n&|<>()"'`%]*[*?][^ \\,\t\r\n&|<>()"'`%]*/,
    )),
    move_directory_destination_text: _ => token(/[^, \\.\\/:\t\r\n&|<>()"'`%]+/),
    xcopy_path_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    type_file_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    call_batch_text: _ => token(/[^ \t\r\n&|<>()"'`%:]+/),
    argument_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    tree_path_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    subst_path_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    sort_file_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    sort_directory_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    sort_locale_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    fc_file_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    comp_file_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    doskey_macro_text_fragment: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    task_value_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    task_format_text: _ => token(/[A-Za-z]+/),
    sc_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    bcdedit_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    compact_target_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    replace_source_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    replace_destination_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    convert_volume_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    chkdsk_target_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    chkntfs_volume_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    print_file_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    icacls_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    cacls_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    recover_target_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    format_volume_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    fsutil_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    label_drive: _ => token(/[A-Za-z]:/),
    label_value_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    robocopy_text: _ => token(/[^ \/\t\r\n&|<>()"'`%]+/),
    robocopy_file_text: _ => token(/[^ \/\t\r\n&|<>()"'`%]+/),
    ftype_command_text: _ => token(/[^ \t\r\n&|<>()"'`%=:;]+|[:;=]/),
    rename_target_text: _ => token(/[^ \\/:\t\r\n&|<>()"'`%]+/),
    rename_source_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    mklink_path_text: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    title_text_fragment: _ => token(/[^ \t\r\n&|<>()"'`%]+/),
    word: _ => token(/[^ \t\r\n&|<>()"'%!=:@]+/),

    _if_operand: $ => choice(
      $.quoted_string,
      $.single_quoted_string,
      $.variable_expansion,
      $.delayed_variable,
      $.for_variable,
      $.path,
      $.number,
      $.word,
    ),

    keyword: _ => token(/[A-Za-z]+/),

    _newline: _ => /\r?\n/,
  },
});
