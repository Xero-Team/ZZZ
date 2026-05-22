;; Schema structure tags

((STag (Name) @type)
 (#match? @type "^(?:[^:]+:)?(?:schema|element|attribute|complexType|simpleType|group|attributeGroup|sequence|choice|all|any|anyAttribute|import|include|redefine|override|annotation|documentation|appinfo|key|keyref|unique|selector|field|simpleContent|complexContent|openContent|alternative)$"))

((ETag (Name) @type)
 (#match? @type "^(?:[^:]+:)?(?:schema|element|attribute|complexType|simpleType|group|attributeGroup|sequence|choice|all|any|anyAttribute|import|include|redefine|override|annotation|documentation|appinfo|key|keyref|unique|selector|field|simpleContent|complexContent|openContent|alternative)$"))

((EmptyElemTag (Name) @type)
 (#match? @type "^(?:[^:]+:)?(?:schema|element|attribute|complexType|simpleType|group|attributeGroup|sequence|choice|all|any|anyAttribute|import|include|redefine|override|annotation|documentation|appinfo|key|keyref|unique|selector|field|simpleContent|complexContent|openContent|alternative)$"))

;; Type construction and facets

((STag (Name) @keyword)
 (#match? @keyword "^(?:[^:]+:)?(?:restriction|extension|list|union|enumeration|pattern|whiteSpace|length|minLength|maxLength|minInclusive|maxInclusive|minExclusive|maxExclusive|totalDigits|fractionDigits|assert|assertion|explicitTimezone|maxScale|minScale|override|defaultOpenContent)$"))

((ETag (Name) @keyword)
 (#match? @keyword "^(?:[^:]+:)?(?:restriction|extension|list|union|enumeration|pattern|whiteSpace|length|minLength|maxLength|minInclusive|maxInclusive|minExclusive|maxExclusive|totalDigits|fractionDigits|assert|assertion|explicitTimezone|maxScale|minScale|override|defaultOpenContent)$"))

((EmptyElemTag (Name) @keyword)
 (#match? @keyword "^(?:[^:]+:)?(?:restriction|extension|list|union|enumeration|pattern|whiteSpace|length|minLength|maxLength|minInclusive|maxInclusive|minExclusive|maxExclusive|totalDigits|fractionDigits|assert|assertion|explicitTimezone|maxScale|minScale|override|defaultOpenContent)$"))

;; Common XSD attribute names

((Attribute (Name) @property)
 (#match? @property "^(?:[^:]+:)?(?:name|type|base|ref|memberTypes|itemType|substitutionGroup|targetNamespace|namespace|schemaLocation|xpath|refer|value|default|fixed|form|use|processContents|minOccurs|maxOccurs|abstract|mixed|nillable|block|final|blockDefault|finalDefault|elementFormDefault|attributeFormDefault|version|source|id|test|mode)$"))

;; Schema-instance attributes

((Attribute (Name) @keyword)
 (#match? @keyword "^(?:xsi:)?(?:type|nil|schemaLocation|noNamespaceSchemaLocation)$"))

;; Numeric attribute values

((Attribute
   (Name) @property
   (AttValue) @number)
 (#match? @property "^(?:[^:]+:)?(?:value|length|minLength|maxLength|minInclusive|maxInclusive|minExclusive|maxExclusive|totalDigits|fractionDigits|minOccurs|maxOccurs|maxScale|minScale)$")
 (#match? @number "^(?:['\"])?\d+(?:\.\d+)?(?:['\"])?$"))

;; Boolean-like attribute values

((Attribute
   (Name) @property
   (AttValue) @boolean)
 (#match? @property "^(?:[^:]+:)?(?:abstract|mixed|nillable)$")
 (#match? @boolean "^(?:['\"])?(?:true|false|0|1)(?:['\"])?$"))

;; Symbolic attribute values used by schema controls

((Attribute
   (Name) @property
   (AttValue) @string.special.symbol)
 (#match? @property "^(?:[^:]+:)?(?:use|processContents|form|mode|blockDefault|finalDefault|elementFormDefault|attributeFormDefault|maxOccurs)$")
 (#match? @string.special.symbol "^(?:['\"])?(?:optional|required|prohibited|skip|lax|strict|qualified|unqualified|interleave|suffix|extension|restriction|substitution|list|union|#all|unbounded)(?:['\"])?$"))

((Attribute
   (Name) @property
   (AttValue) @type)
 (#match? @property "^(?:[^:]+:)?(?:type|base|ref|memberTypes|itemType|substitutionGroup|refer)$")
 (#match? @type "^(?:['\"])?(?:[^\s:\"']+:)?[A-Za-z_][\w.-]*(?:\s+(?:[^\s:\"']+:)?[A-Za-z_][\w.-]*)*(?:['\"])?$"))

;; Built-in datatypes in common XSD attributes

((Attribute
   (Name) @property
   (AttValue) @type.builtin)
 (#match? @property "^(?:[^:]+:)?(?:type|base|memberTypes|itemType)$")
 (#match? @type.builtin "^(?:['\"])?(?:[^:\"'\s]+:)?(?:string|boolean|decimal|float|double|duration|dateTime|time|date|gMonth|gMonthDay|gDay|gYear|gYearMonth|hexBinary|base64Binary|anyURI|QName|NOTATION|normalizedString|token|language|Name|NCName|ID|IDREF|IDREFS|ENTITY|ENTITIES|NMTOKEN|NMTOKENS|integer|nonPositiveInteger|negativeInteger|long|int|short|byte|nonNegativeInteger|unsignedLong|unsignedInt|unsignedShort|unsignedByte|positiveInteger|yearMonthDuration|dayTimeDuration|dateTimeStamp)(?:\s+(?:[^:\"'\s]+:)?(?:string|boolean|decimal|float|double|duration|dateTime|time|date|gMonth|gMonthDay|gDay|gYear|gYearMonth|hexBinary|base64Binary|anyURI|QName|NOTATION|normalizedString|token|language|Name|NCName|ID|IDREF|IDREFS|ENTITY|ENTITIES|NMTOKEN|NMTOKENS|integer|nonPositiveInteger|negativeInteger|long|int|short|byte|nonNegativeInteger|unsignedLong|unsignedInt|unsignedShort|unsignedByte|positiveInteger|yearMonthDuration|dayTimeDuration|dateTimeStamp))*(?:['\"])?$"))
