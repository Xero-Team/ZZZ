const defineGrammar = require("../common/define-grammar");

module.exports = defineGrammar(
  "semicolon",
  ";",
  /[^\"\r\n;]+/,
  /-?(0|[1-9]\d*)[.,]\d+/,
);
