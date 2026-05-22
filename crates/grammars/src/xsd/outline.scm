((element
   (STag
     (Name) @context
     (Attribute
       (Name) @_attribute_name
       (AttValue) @name))) @item
 (#match? @context "^(?:[^:]+:)?(?:element|attribute|complexType|simpleType|group|attributeGroup|key|keyref|unique)$")
 (#eq? @_attribute_name "name"))

((EmptyElemTag
   (Name) @context
   (Attribute
     (Name) @_attribute_name
     (AttValue) @name)) @item
 (#match? @context "^(?:[^:]+:)?(?:element|attribute|complexType|simpleType|group|attributeGroup|key|keyref|unique)$")
 (#eq? @_attribute_name "name"))

((element
   (STag
     (Name) @context
     (Attribute
       (Name) @_attribute_name
       (AttValue) @name))) @item
 (#match? @context "^(?:[^:]+:)?(?:element|attribute|group|attributeGroup)$")
 (#eq? @_attribute_name "ref"))

((EmptyElemTag
   (Name) @context
   (Attribute
     (Name) @_attribute_name
     (AttValue) @name)) @item
 (#match? @context "^(?:[^:]+:)?(?:element|attribute|group|attributeGroup)$")
 (#eq? @_attribute_name "ref"))

((element
   (STag (Name) @name)) @item
 (#match? @name "^(?:[^:]+:)?(?:schema|sequence|choice|all|any|anyAttribute|restriction|extension|list|union|annotation|documentation|appinfo|selector|field|assert|assertion)$"))

((EmptyElemTag (Name) @name) @item
 (#match? @name "^(?:[^:]+:)?(?:schema|sequence|choice|all|any|anyAttribute|restriction|extension|list|union|annotation|documentation|appinfo|selector|field|assert|assertion|minLength|maxLength|minInclusive|maxInclusive|minExclusive|maxExclusive|pattern|enumeration|whiteSpace|length|totalDigits|fractionDigits)$"))
