If the value is a string literal or an expression which evaluates to a
string, a new text file must be created with the string as the file contents.

If the value is an expression that evaluates to a `File` or
`Directory` object, or an array of `File` or `Directory`
objects, this indicates the referenced file or directory
should be added to the designated output directory prior to
executing the tool.

If the value is an expression that evaluates to `null`,
nothing is added to the designated output directory, the entry
has no effect.

If the value is an expression that evaluates to some other
array, number, or object not consisting of `File` or
`Directory` objects, a new file must be created with the value
serialized to JSON text as the file contents. The JSON
serialization behavior should match the behavior of string
interpolation of [Parameter
references](#Parameter_references).
