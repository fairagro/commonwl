Secondary files are specified using the following micro-DSL for secondary files:

- If the value is a string, it is transformed to an object with two fields
  `pattern` and `required`
- By default, the value of `required` is `null`
  (this indicates default behavior, which may be based on the context)
- If the value ends with a question mark `?` the question mark is
  stripped off and the value of the field `required` is set to `False`
- The remaining value is assigned to the field `pattern`

For implementation details and examples, please see
[this section](SchemaSalad.html#Domain_Specific_Language_for_secondary_files)
in the Schema Salad specification.
