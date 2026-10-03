= Modern Typst

#set text(size: 11pt)
#show heading.where(level: 1): it => emph(it.body)
#let (head, ..tail) = (1, 2, 3)
#let card(title, body: []) = block[
  *#title*
  #body
]
#let value = context text.lang
#import "module.typ": (nested.item as renamed, other)
#include "chapter.typ"

#for (key, value) in (("a", 1),) {
  [#key: #value]
}

$ mat(delim: "[", a_1, b^2; ..#(values)) $

```rust
fn main() {}
```
