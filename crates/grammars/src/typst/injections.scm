(raw
  language: (raw_language) @injection.language
  content: (raw_content) @injection.content)

([
  (line_comment)
  (block_comment)
] @injection.content
  (#set! injection.language "comment"))
