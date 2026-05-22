(table
  "["
  (_) @name
  "]") @item

(table_array_element
  "[["
  (_) @name
  "]]") @item

((pair
   (_) @name
   "=") @item
  (#not-has-parent? @item inline_table))
