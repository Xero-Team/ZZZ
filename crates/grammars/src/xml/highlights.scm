;; XML declaration extras

[ "standalone" ] @property

[ "yes" "no" ] @boolean

;; Processing instructions

(XmlModelPI "xml-model" @keyword)

(StyleSheetPI "xml-stylesheet" @keyword)

(PseudoAtt (Name) @property)

(PseudoAtt (PseudoAttValue) @string)

;; Doctype declaration

(doctypedecl "DOCTYPE" @keyword)

(doctypedecl (Name) @type)

;; Tags

(STag (Name) @tag)

(ETag (Name) @tag)

(EmptyElemTag (Name) @tag)

;; Attributes

(Attribute
  (Name) @property
  (#not-match? @property "^xmlns(:|$)"))

(Attribute
  (Name) @keyword
  (#match? @keyword "^xmlns(:|$)"))

(Attribute (AttValue) @string)

;; Delimiters & punctuation

[
 "<?" "?>"
 "<!" "]]>"
 "<" ">"
 "</" "/>"
] @punctuation.delimiter

[ "(" ")" "[" "]" ] @punctuation.bracket

[ "\"" "'" ] @punctuation.delimiter

[ "," "|" "=" ] @operator

;; Text

(CharData) @text

(CDSect
  (CDStart) @punctuation.delimiter
  (CData) @markup.raw
  "]]>" @punctuation.delimiter)

;; Misc

(Comment) @comment

(ERROR) @error
