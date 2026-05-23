/* eslint-disable arrow-parens */
/* eslint-disable-next-line spaced-comment */
/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const timestamp = /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?(?:Z|[+-]\d{2}:\d{2})/;
const printUsAscii = /[!-~]+/;
const sdName = /[^\x00-\x20\x7f="\]]+/;

module.exports = grammar({
  name: "syslog",

  extras: () => [],

  rules: {
    source_file: $ => repeat(choice(seq(prec(1, $.syslog_message), optional($._newline)), seq($.raw_line, optional($._newline)), $._newline)),

    syslog_message: $ => seq(
      $.priority,
      field("version", $.version),
      " ",
      field("timestamp", $.timestamp),
      " ",
      field("hostname", $.hostname),
      " ",
      field("app_name", $.app_name),
      " ",
      field("procid", $.procid),
      " ",
      field("msgid", $.msgid),
      " ",
      field("structured_data", $.structured_data),
      optional(seq(" ", field("message", $.message))),
    ),

    priority: $ => seq("<", $.prival, ">"),
    prival: _ => token(choice("0", /[1-9]\d{0,2}/)),
    version: _ => token(/[1-9]\d{0,2}/),

    timestamp: $ => choice($.nilvalue, $.timestamp_value),
    timestamp_value: _ => token(timestamp),

    hostname: $ => choice($.nilvalue, $.hostname_value),
    hostname_value: _ => token(printUsAscii),

    app_name: $ => choice($.nilvalue, $.app_name_value),
    app_name_value: _ => token(printUsAscii),

    procid: $ => choice($.nilvalue, $.procid_value),
    procid_value: _ => token(printUsAscii),

    msgid: $ => choice($.nilvalue, $.msgid_value),
    msgid_value: _ => token(printUsAscii),

    structured_data: $ => choice($.nilvalue, repeat1($.sd_element)),
    sd_element: $ => seq("[", field("id", $.sd_id), repeat(seq(" ", $.sd_param)), "]"),
    sd_id: _ => token(sdName),
    sd_param: $ => seq(field("name", $.param_name), "=", field("value", $.param_value)),
    param_name: _ => token(sdName),
    param_value: $ => seq('"', repeat(choice($.param_value_fragment, $.escape_sequence)), token.immediate('"')),
    param_value_fragment: _ => token.immediate(/[^"\\\]\r\n]+/),
    escape_sequence: _ => token.immediate(seq("\\", choice('"', "\\", "]"))),

    message: $ => choice($.utf8_message, $.message_text),
    utf8_message: $ => seq($.bom, field("text", $.message_text)),
    bom: _ => "\uFEFF",
    message_text: _ => token(/[^\r\n]+/),

    nilvalue: _ => "-",
    raw_line: _ => token(prec(-1, /[^\r\n]+/)),
    _newline: _ => /\r?\n/,
  },
});
