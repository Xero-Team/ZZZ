module.exports = function defineGrammar(name, delimiter, nonEscapedField, float = /-?(0|[1-9]\d*)\.\d+/) {
  return grammar({
    name,

    rules: {
      source_file: ($) => seq(repeat($._line), optional($.record)),
      _line: ($) => choice(seq($.record, $._eor), $._eor),
      record: ($) => choice($._field, seq(optional($._field), repeat1($._pair))),
      _eor: ($) => choice("\n", "\r\n"),
      _pair: ($) => prec(1, seq(field("sep", $.delimiter), optional($._field))),
      delimiter: ($) => delimiter,
      _field: ($) => choice($.null, $.na, $.boolean, $._number, $.string),

      null: ($) => /null|NULL/,
      na: ($) => /na|NA/,
      boolean: ($) => /true|TRUE|false|FALSE/,
      _number: ($) => choice($.hex, $.float, $.integer),
      integer: ($) => /-?\d+/,
      hex: ($) => /0[xX][\da-fA-F]+/,
      float: ($) => token(prec(1, float)),
      string: ($) => choice(seq($.quote, $.quote), seq($.quote, $.escaped, $.quote), $.non_escaped),
      quote: ($) => '"',
      escaped: ($) => repeat1(prec.left(1, choice($.escape_sequence, $.text))),
      non_escaped: ($) => token(prec(-1, nonEscapedField)),
      text: ($) => /[^\\"\r\n]+/,
      escape_sequence: ($) => token.immediate(prec(10, choice('""', "\n", "\r\n"))),
    },
  });
};
