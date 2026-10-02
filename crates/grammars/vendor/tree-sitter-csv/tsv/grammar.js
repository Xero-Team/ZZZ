const defineGrammar = require("../common/define-grammar");

module.exports = defineGrammar("tsv", "\t", /[^\\"\r\n\t]+/);
