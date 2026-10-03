("(" @open
  ")" @close)

("[" @open
  "]" @close)

("{" @open
  "}" @close)

(string
  "\"" @open
  "\"" @close
  (#set! rainbow.exclude))

(raw
  .
  (raw_delimiter) @open
  (raw_delimiter) @close
  .
  (#set! rainbow.exclude))

(equation
  "$" @open
  "$" @close
  (#set! rainbow.exclude))
