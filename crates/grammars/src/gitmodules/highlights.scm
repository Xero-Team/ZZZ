((section_name) @function.builtin
  (#eq? @function.builtin "submodule"))

((variable
  (name) @keyword)
  (#match? @keyword "^(path|url|branch|update|ignore|fetchRecurseSubmodules|shallow|active)$"))

((variable
  (name) @keyword
  value: (string) @constant.builtin)
  (#eq? @keyword "update")
  (#match? @constant.builtin "^(checkout|rebase|merge|none)$"))

((variable
  (name) @keyword
  value: (string) @constant.builtin)
  (#eq? @keyword "ignore")
  (#match? @constant.builtin "^(all|dirty|untracked|none)$"))

((variable
  (name) @keyword
  value: (string
    (shell_command) @error))
  (#eq? @keyword "update"))
